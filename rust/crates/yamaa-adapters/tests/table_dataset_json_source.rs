//! Table contracts: the Dataset-JSON source profiles' seams that the file
//! route's fixtures (the application suite's
//! `fixtures/diagnostics/dataset_json_sources.json`) cannot
//! reach: the writer's output read back as the table it wrote, the authored
//! Dataset-JSON goldens read back as their published rows, and the
//! narrowing table of DSJSON-2 whole.
mod table {
    mod dataset_json_source {
        use std::path::Path;
        use yamaa_adapters::{
            column_table::{Column, ColumnTable},
            dataset_json_source::{self, Error, Invalid},
            submission_json::{self, DatasetJsonColumn, DatasetJsonDocument, Layout},
        };
        use yamaa_core::{
            conversion::convert,
            specification::{SourceDeclaration, SourceProfile},
            table::{self, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        fn declaration(path: &str, types: Vec<(String, ColumnType)>) -> SourceDeclaration {
            SourceDeclaration {
                name: "SRC".into(),
                path: path.into(),
                types,
                profile: SourceProfile::from_path(path).unwrap(),
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            }
        }

        /// Every cell of a table, row by row, as owned values.
        fn cells<T: TableAccess>(table: &T) -> Vec<Vec<Value>> {
            (0..table.row_count())
                .map(|row| {
                    (0..table.schema().columns().len())
                        .map(|column| match table.cell(row, column) {
                            Ok(ValueRef::Missing) => Value::Missing,
                            Ok(ValueRef::Str(text)) => Value::Str(text.into()),
                            Ok(ValueRef::Int(value)) => Value::Int(value),
                            Ok(ValueRef::Float(value)) => Value::Float(value),
                            Ok(ValueRef::Bool(value)) => Value::Bool(value),
                            Ok(ValueRef::Date(value)) => Value::Date(value),
                            Ok(ValueRef::DateTime(value)) => Value::DateTime(value),
                            Err(_) => panic!("cell {row}, {column}"),
                        })
                        .collect()
                })
                .collect()
        }

        /// A column as the writer declares it: its name, its `dataType` and
        /// its `targetDataType`.
        type Written = (&'static str, &'static str, Option<&'static str>);

        /// The published table the round trip writes: one column per row of
        /// JSON-10's table that a column type reaches, including an ADaM date
        /// the document submits as an integer, with missing values, a present
        /// empty string, the integer bounds, a negative zero and floats whose
        /// shortest text has many digits. A datetime carries its seconds, as
        /// an artifact read under INPUT-11 does.
        fn published() -> (ColumnTable, Vec<Written>) {
            // (name, type, dataType, targetDataType) as JSON-10 maps them.
            let fields: [(&str, ColumnType, &str, Option<&str>); 10] = [
                ("USUBJID", ColumnType::Str, "string", None),
                ("AESEQ", ColumnType::Int, "integer", None),
                ("AVAL", ColumnType::Float, "float", None),
                ("ADT", ColumnType::Date, "date", None),
                ("ADTM", ColumnType::DateTime, "datetime", None),
                ("TRTSDT", ColumnType::Date, "date", Some("integer")),
                ("ATM", ColumnType::Str, "time", None),
                ("AELINK", ColumnType::Str, "URI", None),
                ("AESTDTC", ColumnType::Str, "string", None),
                ("AECOMM", ColumnType::Str, "string", None),
            ];
            let date = |text: &str| Value::Date(text.parse().unwrap());
            let datetime = |text: &str| Value::DateTime(text.parse().unwrap());
            let text = |text: &str| Value::Str(text.into());
            let rows = vec![
                vec![
                    text("S-01"),
                    Value::Int(1),
                    Value::float(172.5),
                    date("2025-01-08"),
                    datetime("2025-01-08T08:30:00"),
                    date("2024-12-31"),
                    text("08:30"),
                    text("https://example.org/s-01"),
                    text("2025-01"),
                    text("say \"hi\"\\\n\tthen \u{0}\u{e9}\u{1f642}/"),
                ],
                vec![
                    text("S-02"),
                    Value::Int(i64::MIN),
                    Value::float(-0.0),
                    Value::Missing,
                    Value::Missing,
                    Value::Missing,
                    Value::Missing,
                    Value::Missing,
                    Value::Missing,
                    text(""),
                ],
                vec![
                    text("S-03"),
                    Value::Int(i64::MAX),
                    Value::float(0.1),
                    date("0001-01-01"),
                    datetime("9999-12-31T23:59:59"),
                    date("1960-01-01"),
                    text(""),
                    text("urn:x"),
                    text("--01-05"),
                    Value::Missing,
                ],
                vec![
                    text("S-04"),
                    Value::Int(0),
                    Value::float(1e21),
                    date("2024-02-29"),
                    datetime("2024-02-29T00:00:00"),
                    date("2024-02-29"),
                    text("23:59:59"),
                    text("x"),
                    text("2025"),
                    text(" leading and trailing "),
                ],
                vec![
                    text("S-05"),
                    Value::Int(-7),
                    Value::float(5e-324),
                    date("2025-03-04"),
                    datetime("2025-03-04T14:05:09"),
                    date("2025-03-04"),
                    text("14:05"),
                    text("y"),
                    text("2025-03-04T14:05"),
                    text("1.50"),
                ],
            ];
            let schema = TableSchema::new(
                fields
                    .iter()
                    .map(|&(name, kind, ..)| table::Column {
                        name: name.into(),
                        kind,
                    })
                    .collect(),
            )
            .unwrap();
            let columns = fields
                .iter()
                .enumerate()
                .map(|(index, &(_, kind, ..))| {
                    let mut column = Column::empty(kind);
                    for row in &rows {
                        column.push(ValueRef::from(&row[index]));
                    }
                    column
                })
                .collect();
            let table = ColumnTable::new(schema, columns, rows.len()).unwrap();
            let types = fields
                .iter()
                .map(|&(name, _, data_type, target)| (name, data_type, target))
                .collect();
            (table, types)
        }

        /// The writer's document for the published table: every member
        /// JSON-7 and JSON-9 write, as a composition would fill them.
        fn document(types: &[Written]) -> DatasetJsonDocument {
            DatasetJsonDocument {
                path: "datasets.ADAE.transport".into(),
                created: "2026-10-24T09:00:00".into(),
                version: "1.1.0".into(),
                file_oid: "FILE.ADAE".into(),
                originator: Some("a sponsor".into()),
                source_name: "yamaa".into(),
                source_version: "1.0.0".into(),
                study_oid: "STDY.X".into(),
                metadata_version_oid: "MDV.X".into(),
                metadata_ref: "define.xml".into(),
                item_group_oid: "IG.ADAE".into(),
                name: "ADAE".into(),
                label: "Adverse Events Analysis Dataset".into(),
                columns: types
                    .iter()
                    .enumerate()
                    .map(|(index, &(name, data_type, target))| DatasetJsonColumn {
                        item_oid: format!("IT.ADAE.{name}"),
                        name: name.into(),
                        label: format!("Label of {name}"),
                        data_type: data_type.into(),
                        target_data_type: target.map(Into::into),
                        length: (data_type == "string").then(|| "200".into()),
                        display_format: target.map(|_| "DATE9.".into()),
                        key_sequence: (index == 0).then_some(1),
                    })
                    .collect(),
            }
        }

        /// JSON-15, DSJSON-2 and DSJSON-3: a file the writer renders for a
        /// published table, in either representation, reads back as that
        /// table, field for field, value for value and type for type. The
        /// input declares `empty_string: present`, the convention under which
        /// the present empty string the table holds stays one.
        #[test]
        fn the_writers_output_for_a_published_table_reads_back_as_that_table() {
            let (table, types) = published();
            let document = document(&types);
            for (layout, path) in [
                (Layout::Object, "adae.json"),
                (Layout::Lines, "adae.ndjson"),
            ] {
                let bytes = submission_json::render(&document, &table, layout).unwrap();
                let mut source = declaration(path, vec![]);
                source.empty_string_present = true;
                let read = dataset_json_source::parse(&bytes, &source)
                    .unwrap_or_else(|error| panic!("{path}: {error:?}"));
                assert_eq!(
                    read.schema().columns(),
                    table.schema().columns(),
                    "{path}: fields and types"
                );
                assert_eq!(cells(&read), cells(&table), "{path}: values");
                // A negative zero keeps its sign.
                assert!(
                    matches!(read.cell(1, 2), Ok(ValueRef::Float(value)) if value.get().is_sign_negative()),
                    "{path}: negative zero"
                );
            }
        }

        /// JSON-15: under the default convention, a written present empty
        /// string reads as missing, as from any source (INPUT-9), and a `str`
        /// column a document submits as a date reads as one unless the input
        /// declares it `str` (DSJSON-2).
        #[test]
        fn a_written_empty_string_and_a_text_date_read_back_under_the_input_s_declarations() {
            let (table, mut types) = published();
            // The partial-date column, submitted as `date`, holds only
            // complete dates and missing values in rows the test keeps.
            types[8].1 = "date";
            let kept = table.rows(0..2).unwrap();
            let bytes = submission_json::render(&document(&types), &kept, Layout::Object).unwrap();
            let read = dataset_json_source::parse(&bytes, &declaration("adae.json", vec![]));
            assert!(
                matches!(
                    &read,
                    Err(Error::Value { field, target: ColumnType::Date, value, record: 1, declared: false })
                        if field == "AESTDTC" && value == "2025-01"
                ),
                "a partial date is no date: {read:?}"
            );
            let read = dataset_json_source::parse(
                &bytes,
                &declaration("adae.json", vec![("AESTDTC".into(), ColumnType::Str)]),
            )
            .unwrap();
            assert_eq!(read.schema().columns()[8].kind, ColumnType::Str);
            let mut expected = cells(&kept);
            expected[1][9] = Value::Missing;
            assert_eq!(cells(&read), expected, "declared str and default missing");
        }

        /// JSON-15: every authored Dataset-JSON golden of a submission
        /// benchmark reads back as the rows of the published artifact beside
        /// it, each value written as CSV-5 writes its type and each missing
        /// value as an empty field.
        #[test]
        fn every_dataset_json_golden_reads_back_as_its_published_rows() {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            let mut read = 0;
            let mut cases = std::fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            cases.sort();
            for case in cases {
                let Ok(expected) = std::fs::read_dir(case.join("expected")) else {
                    continue;
                };
                for golden in expected.map(|entry| entry.unwrap().path()) {
                    let name = golden.file_name().unwrap().to_str().unwrap().to_owned();
                    let Some(stem) = name
                        .strip_suffix(".json")
                        .or_else(|| name.strip_suffix(".ndjson"))
                    else {
                        continue;
                    };
                    let bytes = std::fs::read(&golden).unwrap();
                    if !bytes.windows(18).any(|w| w == b"datasetJSONVersion") {
                        continue;
                    }
                    let label = format!("{}/{name}", case.display());
                    let table = dataset_json_source::parse(&bytes, &declaration(&name, vec![]))
                        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
                    let csv = std::fs::read(golden.with_file_name(format!("{stem}.csv"))).unwrap();
                    let published = yamaa_adapters::csv_source::parse(&csv).unwrap();
                    let names = table
                        .schema()
                        .columns()
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>();
                    assert_eq!(names, published.names, "{label}: fields");
                    let rows = cells(&table)
                        .into_iter()
                        .map(|row| {
                            row.into_iter()
                                .map(|value| match convert(&value, ColumnType::Str).unwrap() {
                                    Value::Missing => None,
                                    Value::Str(text) => Some(text),
                                    other => panic!("{label}: text {other:?}"),
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(rows, published.records, "{label}: rows");
                    read += 1;
                }
            }
            assert_eq!(read, 5, "the five Dataset-JSON goldens");
        }

        /// DSJSON-2: a `types` entry may restate the type a `dataType`
        /// supplies, or narrow as the table states; every other pair is
        /// refused as `field_type`, naming the field, its `dataType` and the
        /// declared type.
        #[test]
        fn a_types_entry_narrows_exactly_as_the_narrowing_table_states() {
            use ColumnType::{Date, DateTime, Float, Int, Str};
            let every = [Str, Int, Float, Date, DateTime];
            // Each dataType and the types a `types` entry may declare for it.
            let table: [(&str, &[ColumnType]); 9] = [
                ("string", &[Str, Int, Float, Date, DateTime]),
                ("URI", &[Str, Int, Float, Date, DateTime]),
                ("time", &[Str, Int, Float, Date, DateTime]),
                ("integer", &[Int]),
                ("float", &[Float, Int]),
                ("double", &[Float, Int]),
                ("decimal", &[Float, Int, Str]),
                ("date", &[Date, Str]),
                ("datetime", &[DateTime, Str]),
            ];
            for (data_type, admitted) in table {
                for declared in every {
                    let bytes = format!(
                        r#"{{"datasetJSONCreationDateTime": "2026-10-24T09:00:00", "datasetJSONVersion": "1.1.0", "itemGroupOID": "IG.X", "records": 0, "name": "X", "label": "X", "columns": [{{"itemOID": "IT.X.F", "name": "F", "label": "F", "dataType": "{data_type}"}}], "rows": []}}"#
                    );
                    let read = dataset_json_source::parse(
                        bytes.as_bytes(),
                        &declaration("x.json", vec![("F".into(), declared)]),
                    );
                    let label = format!("{data_type} declared {declared:?}");
                    if admitted.contains(&declared) {
                        let table = read.unwrap_or_else(|error| panic!("{label}: {error:?}"));
                        assert_eq!(table.schema().columns()[0].kind, declared, "{label}");
                    } else {
                        match read {
                            Err(Error::Invalid(Invalid::FieldType {
                                field,
                                data_type: written,
                                declared: refused,
                            })) => assert_eq!(
                                (field.as_str(), written.as_str(), refused),
                                ("F", data_type, declared),
                                "{label}"
                            ),
                            other => panic!("{label}: {other:?}"),
                        }
                    }
                }
            }
        }
    }
}
