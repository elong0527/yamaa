//! Table contracts: CSV and Parquet inputs read by partition (#3792). A CSV
//! input is scanned once and decoded a range of records at a time, a Parquet
//! input a run of whole row groups at a time, each partition admitted on its
//! own; the partitions hold the whole read's table, and every finding is the
//! whole read's. The scan decodes each partition once and drops it, but for
//! a Parquet input's first partitions of up to one partition's records,
//! which it keeps for the run (#4212), and any range of records then
//! decodes from what it kept and, for the rest, again from the held bytes
//! (#3830).
//!
//! The seam is the decoder port's read by partition, which neither the
//! benchmark reader nor the host fixtures reach: no host runs partitioned
//! before #3794.
mod table {
    mod partitioned_sources {
        use std::convert::Infallible;
        use std::num::NonZeroUsize;
        use yamaa_adapters::csv_source::{self, Error as CsvError, Field, ProfileCause};
        use yamaa_adapters::decoder::{Decoder, Snapshot};
        use yamaa_adapters::source_decoding::Error;
        use yamaa_core::decoder::Decoder as _;
        use yamaa_core::specification::{SourceDeclaration, SourceInterpretation, SourceProfile};
        use yamaa_core::table::{Column, TableAccess, TableSchema, ValueRef};
        use yamaa_core::value::ColumnType;

        fn declaration(
            profile: SourceProfile,
            types: &[(&str, ColumnType)],
            ordinal: Option<&str>,
        ) -> SourceDeclaration {
            SourceDeclaration {
                name: "SRC".into(),
                path: match profile {
                    SourceProfile::Parquet => "input.parquet".into(),
                    _ => "input.csv".into(),
                },
                types: types
                    .iter()
                    .map(|&(name, kind)| (name.into(), kind))
                    .collect(),
                profile,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: ordinal.map(Into::into),
            }
        }

        fn size(records: usize) -> NonZeroUsize {
            NonZeroUsize::new(records).unwrap()
        }

        /// An input scanned by partition (#3830): the table the scan left,
        /// which holds no record, the records of each partition it visited,
        /// and each partition decoded again from its range.
        struct Divided {
            scanned: Snapshot<Infallible>,
            sizes: Vec<usize>,
            parts: Vec<Snapshot<Infallible>>,
        }

        /// The input read whole, then scanned by partitions of `records`.
        type Reads = (Result<Snapshot<Infallible>, Error>, Result<Divided, Error>);
        fn read(
            source: &SourceDeclaration,
            bytes: &[u8],
            interpretation: SourceInterpretation,
            records: usize,
        ) -> Reads {
            let mut decoder = Decoder::<Infallible>::default();
            let whole = decoder.table(source, bytes, interpretation, None);
            let mut visited = Vec::new();
            let scanned = decoder
                .partitioned(
                    source,
                    bytes,
                    interpretation,
                    None,
                    size(records),
                    &mut |part, first| visited.push(first..first + part.row_count()),
                )
                .expect("a CSV or Parquet input scans by partition");
            let divided = scanned.map(|scanned| Divided {
                sizes: visited.iter().map(|range| range.len()).collect(),
                parts: visited
                    .iter()
                    .map(|range| {
                        decoder
                            .partition(&scanned, source, bytes, interpretation, None, range.clone())
                            .unwrap()
                    })
                    .collect(),
                scanned,
            });
            (whole, divided)
        }

        /// The records of `parts`, in order, cell by cell.
        fn joined(parts: &[Snapshot<Infallible>]) -> Vec<Vec<ValueRef<'_>>> {
            parts.iter().flat_map(|part| cells(part)).collect()
        }

        /// A table's records, cell by cell.
        fn cells(table: &dyn TableAccess<Error = Infallible>) -> Vec<Vec<ValueRef<'_>>> {
            (0..table.row_count())
                .map(|row| {
                    (0..table.schema().columns().len())
                        .map(|column| table.cell(row, column).unwrap())
                        .collect()
                })
                .collect()
        }

        fn schema(columns: &[(&str, ColumnType)]) -> TableSchema {
            TableSchema::new(
                columns
                    .iter()
                    .map(|&(name, kind)| Column {
                        name: name.into(),
                        kind,
                    })
                    .collect(),
            )
            .unwrap()
        }

        mod csv {
            use super::*;
            use ValueRef::{Int, Missing, Str};

            /// Quoted fields hold line feeds, doubled quotes and commas, and
            /// records end with a line feed or a carriage return and line
            /// feed: the scan ends each record outside its quotes, so every
            /// partition boundary falls between two records, and the
            /// partitions hold the whole read's table (CSV-2, CSV-3, CSV-5).
            #[test]
            fn quoted_multiline_records_read_by_partition_equal_the_whole_read() {
                let bytes = concat!(
                    "ID,N,NOTE\r\n",
                    "1,10,\"first\nline, second\"\r\n",
                    "2,20,\"\"\"quoted\"\"\n\n\"\n",
                    "3,,\"\"\r\n",
                    "4,40,\"a\nb\nc\"\n",
                    "5,50,plain",
                );
                let expected = vec![
                    vec![Str("1"), Int(10), Str("first\nline, second")],
                    vec![Str("2"), Int(20), Str("\"quoted\"\n\n")],
                    vec![Str("3"), Missing, Missing],
                    vec![Str("4"), Int(40), Str("a\nb\nc")],
                    vec![Str("5"), Int(50), Str("plain")],
                ];
                let source = declaration(SourceProfile::Csv, &[("N", ColumnType::Int)], None);
                for (records, partitions) in [
                    (1, vec![1, 1, 1, 1, 1]),
                    (2, vec![2, 2, 1]),
                    (3, vec![3, 2]),
                    (4, vec![4, 1]),
                    (5, vec![5]),
                    (6, vec![5]),
                ] {
                    let (whole, divided) = read(
                        &source,
                        bytes.as_bytes(),
                        SourceInterpretation::Ordinary,
                        records,
                    );
                    let (whole, divided) = (whole.unwrap(), divided.unwrap());
                    assert_eq!(cells(&whole), expected, "the whole read");
                    assert_eq!(joined(&divided.parts), expected, "partitions of {records}");
                    assert_eq!(divided.sizes, partitions, "partitions of {records}");
                    assert_eq!(
                        divided.scanned.schema(),
                        &schema(&[
                            ("ID", ColumnType::Str),
                            ("N", ColumnType::Int),
                            ("NOTE", ColumnType::Str)
                        ]),
                        "partitions of {records}"
                    );
                }
            }

            /// A range of records decodes as the whole read gives them,
            /// whether its first and last records fall in one partition of
            /// the scan or in several (#3830): a key-bounded cut need not
            /// fall where the scan's partitions of two records do.
            #[test]
            fn a_range_of_records_decodes_as_the_whole_read_across_the_scans_partitions() {
                let bytes = b"ID,NOTE\n1,\"a\nb\"\n2,c\n3,\"d,e\"\n4,f\n5,g\n";
                let expected = [
                    vec![Str("1"), Str("a\nb")],
                    vec![Str("2"), Str("c")],
                    vec![Str("3"), Str("d,e")],
                    vec![Str("4"), Str("f")],
                    vec![Str("5"), Str("g")],
                ];
                let source = declaration(SourceProfile::Csv, &[], None);
                let mut decoder = Decoder::<Infallible>::default();
                let scanned = decoder
                    .partitioned(
                        &source,
                        bytes,
                        SourceInterpretation::Ordinary,
                        None,
                        size(2),
                        &mut |_, _| {},
                    )
                    .unwrap()
                    .unwrap();
                for range in [0..5, 1..4, 1..2, 2..4, 3..5, 4..5, 2..2] {
                    let part = decoder
                        .partition(
                            &scanned,
                            &source,
                            bytes,
                            SourceInterpretation::Ordinary,
                            None,
                            range.clone(),
                        )
                        .unwrap();
                    assert_eq!(cells(&part), expected[range.clone()], "records {range:?}");
                }
            }

            /// The blank lines that end a source are no record of any
            /// partition, and a blank line before the last record is an
            /// empty record of its own, wherever the partitions fall
            /// (CSV-11): every partition size, and every range of records
            /// decoded again, holds the whole read's table.
            #[test]
            fn trailing_blank_lines_are_no_record_of_any_partition() {
                let source = declaration(SourceProfile::Csv, &[], None);
                for (bytes, expected) in [
                    (
                        &b"ID\n1\n\n2\n3\n\r\n\n"[..],
                        vec![
                            vec![Str("1")],
                            vec![Missing],
                            vec![Str("2")],
                            vec![Str("3")],
                        ],
                    ),
                    (
                        b"A,B\r\n1,x\r\n2,y\r\n3,z\r\n\r\n\r\n",
                        vec![
                            vec![Str("1"), Str("x")],
                            vec![Str("2"), Str("y")],
                            vec![Str("3"), Str("z")],
                        ],
                    ),
                ] {
                    let count = expected.len();
                    for records in 1..=count + 1 {
                        let (whole, divided) =
                            read(&source, bytes, SourceInterpretation::Ordinary, records);
                        let (whole, divided) = (whole.unwrap(), divided.unwrap());
                        let sizes = (0..count)
                            .step_by(records)
                            .map(|first| records.min(count - first))
                            .collect::<Vec<_>>();
                        let case = format!("{bytes:?}, partitions of {records}");
                        assert_eq!(cells(&whole), expected, "the whole read: {case}");
                        assert_eq!(joined(&divided.parts), expected, "{case}");
                        assert_eq!(divided.sizes, sizes, "{case}");
                        let mut decoder = Decoder::<Infallible>::default();
                        for first in 0..count {
                            let range = first..count;
                            let part = decoder
                                .partition(
                                    &divided.scanned,
                                    &source,
                                    bytes,
                                    SourceInterpretation::Ordinary,
                                    None,
                                    range.clone(),
                                )
                                .unwrap();
                            assert_eq!(cells(&part), expected[range.clone()], "{case}: {range:?}");
                        }
                    }
                }
            }

            fn profile(cause: ProfileCause, record: usize, field: usize) -> CsvError {
                CsvError::Profile {
                    cause,
                    record,
                    field: Field::Index(field),
                }
            }

            /// A defect past the first partition of two records fails the
            /// scan with the whole read's finding: its condition and its
            /// record and field, the header being record 1. Complete syntax
            /// precedes a width finding, and decoding precedes syntax, as in
            /// the whole read.
            #[test]
            fn a_malformed_record_past_the_first_partition_fails_as_the_whole_read_does() {
                use ProfileCause::*;
                for (input, expected, case) in [
                    (
                        &b"A,B\n1,x\n2,y\n3,\"z\n"[..],
                        profile(QuoteUnterminated, 4, 2),
                        "an unterminated quote",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,z\rw\n",
                        profile(CarriageReturn, 4, 2),
                        "a carriage return in a bare field",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,\"z\r\"\n",
                        profile(CarriageReturn, 4, 2),
                        "a carriage return in a quoted field",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,\"z\n\r\n",
                        profile(CarriageReturn, 4, 2),
                        "a CR LF after a line feed in an unclosed quote",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,\"z\n\n",
                        profile(QuoteUnterminated, 4, 2),
                        "blank lines in an unclosed quote",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,\"z\"w\n",
                        profile(TextAfterQuote, 4, 2),
                        "text after a closing quote",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,z\"w\n",
                        profile(QuoteInBareField, 4, 2),
                        "a quote in a bare field",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3\n",
                        profile(RecordWidth, 4, 2),
                        "a record one field short",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3,z,w\n",
                        profile(RecordWidth, 4, 3),
                        "a record one field long",
                    ),
                    (
                        b"A,B\n1,x\n2,y\n3\n4\n",
                        profile(RecordWidth, 4, 2),
                        "the first of two width findings",
                    ),
                    (
                        b"A,B\n1\n2,y\n3,\"z\n",
                        profile(QuoteUnterminated, 4, 2),
                        "syntax after a width finding in the first partition",
                    ),
                    (
                        b"A,B\n1,\"x\ny\"\n2,y\n3,\"z\n",
                        profile(QuoteUnterminated, 4, 2),
                        "a multi-line record before the defect",
                    ),
                    (
                        b"A,B\n1,\"x\ny\"\n2,y\n3,\xff\n",
                        profile(InvalidText, 4, 2),
                        "invalid text after a multi-line record",
                    ),
                ] {
                    assert_eq!(
                        csv_source::parse_declared(input, false),
                        Err(expected.clone()),
                        "the whole read: {case}"
                    );
                    assert_eq!(
                        csv_source::scan(input, false, size(2)).err(),
                        Some(expected),
                        "partitions of 2: {case}"
                    );
                }
            }

            /// A declared field whose value does not convert, past the first
            /// partition, fails both reads with the same field, type and
            /// value; every record before it converts first, in both.
            #[test]
            fn a_value_that_does_not_convert_past_the_first_partition_fails_as_the_whole_read_does()
            {
                let source = declaration(SourceProfile::Csv, &[("N", ColumnType::Int)], None);
                let bytes = b"ID,N\n1,1\n2,2\n3,3\n4,four\n5,five\n";
                let (whole, divided) = read(&source, bytes, SourceInterpretation::Ordinary, 2);
                for (error, case) in [
                    (whole.err(), "the whole read"),
                    (divided.err(), "partitions of 2"),
                ] {
                    match error {
                        Some(Error::TypedSource(yamaa_adapters::typed_csv::Error::Typing(
                            yamaa_core::typed_csv::Error::FieldParse {
                                field,
                                target,
                                value,
                            },
                        ))) => assert_eq!(
                            (field.as_str(), target, value.as_str()),
                            ("N", ColumnType::Int, "four"),
                            "{case}"
                        ),
                        other => panic!("{case}: {other:?}"),
                    }
                }
            }

            /// A source of a header alone, or of a header and the blank
            /// lines that end the source (CSV-11), is one empty partition
            /// with the header's schema.
            #[test]
            fn a_source_without_records_is_one_empty_partition() {
                let source = declaration(SourceProfile::Csv, &[], None);
                for bytes in [
                    &b"A,B"[..],
                    b"A,B\n",
                    b"A,B\r\n",
                    b"A,B\n\n",
                    b"A,B\r\n\r\n\n",
                ] {
                    let (_, divided) = read(&source, bytes, SourceInterpretation::Ordinary, 3);
                    let divided = divided.unwrap();
                    assert_eq!(divided.sizes, [0], "{bytes:?}");
                    assert_eq!(
                        divided.scanned.schema(),
                        &schema(&[("A", ColumnType::Str), ("B", ColumnType::Str)]),
                        "{bytes:?}"
                    );
                }
            }

            /// An `odm` read binds the header's fields once, and each
            /// partition holds the fixed table's records (NAME-8).
            #[test]
            fn an_odm_source_read_by_partition_holds_the_whole_reads_records() {
                use yamaa_core::odm_source::FIELDS;
                let header = FIELDS.join(",");
                let record =
                    |subject: usize| format!("S,M,{subject},E,1,G,1,I,1,IT.AGE,{subject}0\n");
                let bytes = format!("{header}\n{}{}{}", record(1), record(2), record(3));
                let source = declaration(SourceProfile::Csv, &[], None);
                let (whole, divided) =
                    read(&source, bytes.as_bytes(), SourceInterpretation::Odm, 2);
                let (whole, divided) = (whole.unwrap(), divided.unwrap());
                assert_eq!(divided.sizes, [2, 1]);
                assert_eq!(divided.scanned.schema(), whole.schema());
                assert_eq!(joined(&divided.parts), cells(&whole));
                assert_eq!(divided.parts[1].cell(0, 2).unwrap(), Str("3"));
                assert_eq!(divided.parts[1].cell(0, 10).unwrap(), Str("30"));
            }
        }

        mod row_groups {
            #![cfg(feature = "parquet")]
            use super::*;
            use ::parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
            use arrow_array::{ArrayRef, Date32Array, Int64Array, RecordBatch, StringArray};
            use arrow_schema::{Field as ArrowField, Schema};
            use std::sync::Arc;

            /// A file of `columns`, written in row groups of `group` rows.
            fn written(columns: Vec<(&str, ArrayRef)>, group: usize) -> Vec<u8> {
                let schema = Arc::new(Schema::new(
                    columns
                        .iter()
                        .map(|(name, array)| {
                            ArrowField::new(*name, array.data_type().clone(), true)
                        })
                        .collect::<Vec<_>>(),
                ));
                let batch = RecordBatch::try_new(
                    schema.clone(),
                    columns.into_iter().map(|(_, array)| array).collect(),
                )
                .unwrap();
                let properties = WriterProperties::builder()
                    .set_max_row_group_size(group)
                    .build();
                let mut bytes = Vec::new();
                let mut writer =
                    ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                bytes
            }

            /// Fifteen records `ID` 1 to 15 and `NAME` r1 to r15, in five
            /// row groups of three.
            fn fifteen() -> Vec<u8> {
                written(
                    vec![
                        ("ID", Arc::new(Int64Array::from_iter_values(1..=15))),
                        (
                            "NAME",
                            Arc::new(StringArray::from_iter_values(
                                (1..=15).map(|id| format!("r{id}")),
                            )),
                        ),
                    ],
                    3,
                )
            }

            /// A partition takes whole row groups while they fit in its
            /// records, and a row group of more records is a partition of
            /// its own; the partitions hold the whole read's table.
            #[test]
            fn a_parquet_input_is_read_a_run_of_whole_row_groups_at_a_time() {
                let names: Vec<String> = (1..=15).map(|id| format!("r{id}")).collect();
                let expected: Vec<Vec<ValueRef<'_>>> = (0..15)
                    .map(|row| vec![ValueRef::Int(row as i64 + 1), ValueRef::Str(&names[row])])
                    .collect();
                let source = declaration(SourceProfile::Parquet, &[], None);
                let bytes = fifteen();
                for (records, partitions) in [
                    (1, vec![3, 3, 3, 3, 3]),
                    (3, vec![3, 3, 3, 3, 3]),
                    (5, vec![3, 3, 3, 3, 3]),
                    (6, vec![6, 6, 3]),
                    (8, vec![6, 6, 3]),
                    (9, vec![9, 6]),
                    (15, vec![15]),
                    (16, vec![15]),
                ] {
                    let (whole, divided) =
                        read(&source, &bytes, SourceInterpretation::Ordinary, records);
                    let (whole, divided) = (whole.unwrap(), divided.unwrap());
                    assert_eq!(cells(&whole), expected, "the whole read");
                    assert_eq!(joined(&divided.parts), expected, "partitions of {records}");
                    assert_eq!(divided.sizes, partitions, "partitions of {records}");
                }
            }

            /// A range of records decodes from the row groups that hold
            /// them, as the whole read gives those records, whether it
            /// covers whole row groups or parts of them (#3830).
            #[test]
            fn a_range_of_records_decodes_as_the_whole_read_from_its_row_groups() {
                let names: Vec<String> = (1..=15).map(|id| format!("r{id}")).collect();
                let expected: Vec<Vec<ValueRef<'_>>> = (0..15)
                    .map(|row| vec![ValueRef::Int(row as i64 + 1), ValueRef::Str(&names[row])])
                    .collect();
                let source = declaration(SourceProfile::Parquet, &[], None);
                let bytes = fifteen();
                let mut decoder = Decoder::<Infallible>::default();
                let scanned = decoder
                    .partitioned(
                        &source,
                        &bytes,
                        SourceInterpretation::Ordinary,
                        None,
                        size(6),
                        &mut |_, _| {},
                    )
                    .unwrap()
                    .unwrap();
                for range in [0..15, 0..6, 2..7, 3..6, 5..15, 14..15, 7..7] {
                    let part = decoder
                        .partition(
                            &scanned,
                            &source,
                            &bytes,
                            SourceInterpretation::Ordinary,
                            None,
                            range.clone(),
                        )
                        .unwrap();
                    assert_eq!(cells(&part), expected[range.clone()], "records {range:?}");
                }
            }

            /// A range keeps only its own records of the row groups it
            /// reads, their missing values where they are: `NAME` is missing
            /// for every third `ID` and `X` for every fourth, so ranges that
            /// start and end inside a row group of three begin and end on
            /// missing and present values alike (#4200).
            #[test]
            fn a_range_keeps_the_missing_values_of_its_records() {
                use arrow_array::Float64Array;
                let name = |id: i64| (id % 3 != 0).then(|| format!("r{id}"));
                let half = |id: i64| (id % 4 != 0).then_some(id as f64 / 2.0);
                let bytes = written(
                    vec![
                        ("ID", Arc::new(Int64Array::from_iter_values(1..=15))),
                        ("NAME", Arc::new(StringArray::from_iter((1..=15).map(name)))),
                        ("X", Arc::new(Float64Array::from_iter((1..=15).map(half)))),
                    ],
                    3,
                );
                let names: Vec<Option<String>> = (1..=15).map(name).collect();
                let expected: Vec<Vec<ValueRef<'_>>> = (1..=15i64)
                    .map(|id| {
                        let row = id as usize - 1;
                        vec![
                            ValueRef::Int(id),
                            names[row]
                                .as_deref()
                                .map_or(ValueRef::Missing, ValueRef::Str),
                            half(id).map_or(ValueRef::Missing, |x| {
                                ValueRef::Float(yamaa_core::value::FiniteFloat::new(x).unwrap())
                            }),
                        ]
                    })
                    .collect();
                let source = declaration(SourceProfile::Parquet, &[], None);
                let mut decoder = Decoder::<Infallible>::default();
                let scanned = decoder
                    .partitioned(
                        &source,
                        &bytes,
                        SourceInterpretation::Ordinary,
                        None,
                        size(6),
                        &mut |_, _| {},
                    )
                    .unwrap()
                    .unwrap();
                for range in [0..15, 2..7, 3..4, 4..5, 5..14, 11..12, 13..15, 7..7] {
                    let part = decoder
                        .partition(
                            &scanned,
                            &source,
                            &bytes,
                            SourceInterpretation::Ordinary,
                            None,
                            range.clone(),
                        )
                        .unwrap();
                    assert_eq!(cells(&part), expected[range.clone()], "records {range:?}");
                }
            }

            /// A run's partitions read the records the scan kept, the first
            /// partition of six records in row groups of three (#4212), and
            /// decode the rest again: a division whose ranges start and end
            /// inside the kept partition and the row groups, decoded in
            /// either order, on the calling thread or through the decode
            /// threads share, gives the whole read's records, with `NAME`
            /// missing for every third `ID`. A range decoded once more, as
            /// a partition that did not decode on its thread is, gives
            /// them again from the held bytes.
            #[test]
            fn a_division_reads_the_kept_partition_and_decodes_the_rest_as_the_whole_read() {
                let name = |id: i64| (id % 3 != 0).then(|| format!("r{id}"));
                let bytes = written(
                    vec![
                        ("ID", Arc::new(Int64Array::from_iter_values(1..=15))),
                        ("NAME", Arc::new(StringArray::from_iter((1..=15).map(name)))),
                    ],
                    3,
                );
                let names: Vec<Option<String>> = (1..=15).map(name).collect();
                let expected: Vec<Vec<ValueRef<'_>>> = (1..=15i64)
                    .map(|id| {
                        let name = names[id as usize - 1].as_deref();
                        vec![
                            ValueRef::Int(id),
                            name.map_or(ValueRef::Missing, ValueRef::Str),
                        ]
                    })
                    .collect();
                let source = declaration(SourceProfile::Parquet, &[], None);
                let interpretation = SourceInterpretation::Ordinary;
                let divisions: [(&str, Vec<std::ops::Range<usize>>); 4] = [
                    ("in order", vec![0..4, 4..9, 9..15]),
                    ("later first", vec![4..9, 9..15, 0..4]),
                    ("the kept partition whole", vec![0..6, 6..15]),
                    ("a range across every partition", vec![0..14, 14..15]),
                ];
                for (label, ranges) in divisions {
                    for shared in [false, true] {
                        let mut decoder = Decoder::<Infallible>::default();
                        let scanned = decoder
                            .partitioned(
                                &source,
                                &bytes,
                                interpretation,
                                None,
                                size(6),
                                &mut |_, _| {},
                            )
                            .unwrap()
                            .unwrap();
                        let mut decode = |range: std::ops::Range<usize>| match shared {
                            true => decoder
                                .shared_partition(&scanned, &source, &bytes, interpretation, None)
                                .unwrap()(range),
                            false => decoder.partition(
                                &scanned,
                                &source,
                                &bytes,
                                interpretation,
                                None,
                                range,
                            ),
                        };
                        for range in ranges.iter().chain(ranges.first()) {
                            let part = decode(range.clone()).unwrap();
                            assert_eq!(
                                cells(&part),
                                expected[range.clone()],
                                "{label}, shared {shared}: records {range:?}"
                            );
                        }
                    }
                }
            }

            /// A stored date outside the years 1 to 9999 in the third row
            /// group fails both reads at its row in the whole file, 8, not
            /// at its row in the partition (PARQUET-5).
            #[test]
            fn a_parquet_value_past_the_first_partition_fails_at_its_row_in_the_file() {
                let mut days: Vec<Option<i32>> = (0..9).map(Some).collect();
                days[7] = Some(2_932_897);
                let bytes = written(
                    vec![("D", Arc::new(Date32Array::from(days)) as ArrayRef)],
                    3,
                );
                let source = declaration(SourceProfile::Parquet, &[], None);
                let (whole, divided) = read(&source, &bytes, SourceInterpretation::Ordinary, 3);
                for (error, case) in [
                    (whole.err(), "the whole read"),
                    (divided.err(), "partitions of 3"),
                ] {
                    match error {
                        Some(Error::ParquetSource(
                            yamaa_adapters::parquet_source::Error::Value { field, row, value },
                        )) => {
                            assert_eq!((field.as_str(), row, value), ("D", 8, 2_932_897), "{case}")
                        }
                        other => panic!("{case}: {other:?}"),
                    }
                }
            }

            /// Out-of-range dates in two columns: the second column's in the
            /// first partition, the first column's in the second. The whole
            /// read checks each column over every record before the next
            /// column, so both reads report the first column's, at its row in
            /// the file, 5 (PARQUET-5, #3830).
            #[test]
            fn a_parquet_finding_in_an_earlier_column_of_a_later_partition_comes_first() {
                let mut first: Vec<Option<i32>> = (0..6).map(Some).collect();
                let mut second = first.clone();
                first[4] = Some(2_932_897);
                second[1] = Some(2_932_897);
                let bytes = written(
                    vec![
                        ("A", Arc::new(Date32Array::from(first)) as ArrayRef),
                        ("B", Arc::new(Date32Array::from(second)) as ArrayRef),
                    ],
                    3,
                );
                let source = declaration(SourceProfile::Parquet, &[], None);
                let (whole, divided) = read(&source, &bytes, SourceInterpretation::Ordinary, 3);
                for (error, case) in [
                    (whole.err(), "the whole read"),
                    (divided.err(), "partitions of 3"),
                ] {
                    match error {
                        Some(Error::ParquetSource(
                            yamaa_adapters::parquet_source::Error::Value { field, row, value },
                        )) => {
                            assert_eq!((field.as_str(), row, value), ("A", 5, 2_932_897), "{case}")
                        }
                        other => panic!("{case}: {other:?}"),
                    }
                }
            }

            /// A file of no rows is one empty partition with its schema.
            #[test]
            fn a_parquet_input_without_rows_is_one_empty_partition() {
                let bytes = written(
                    vec![(
                        "ID",
                        Arc::new(Int64Array::from(Vec::<i64>::new())) as ArrayRef,
                    )],
                    3,
                );
                let source = declaration(SourceProfile::Parquet, &[], None);
                let (_, divided) = read(&source, &bytes, SourceInterpretation::Ordinary, 2);
                let divided = divided.unwrap();
                assert_eq!(divided.sizes, [0]);
                assert_eq!(
                    divided.scanned.schema(),
                    &schema(&[("ID", ColumnType::Int)])
                );
            }

            /// An `odm` read of a file in row groups binds the eleven fields
            /// from the footer once, and each partition holds the whole
            /// read's records.
            #[test]
            fn an_odm_parquet_input_read_by_partition_holds_the_whole_reads_records() {
                use yamaa_core::odm_source::FIELDS;
                let columns = FIELDS
                    .iter()
                    .map(|name| {
                        let values = StringArray::from_iter_values(
                            (1..=5).map(|record| format!("{name}-{record}")),
                        );
                        (*name, Arc::new(values) as ArrayRef)
                    })
                    .collect();
                let bytes = written(columns, 2);
                let source = declaration(SourceProfile::Parquet, &[], None);
                let (whole, divided) = read(&source, &bytes, SourceInterpretation::Odm, 2);
                let (whole, divided) = (whole.unwrap(), divided.unwrap());
                assert_eq!(divided.sizes, [2, 2, 1]);
                assert_eq!(divided.scanned.schema(), whole.schema());
                assert_eq!(joined(&divided.parts), cells(&whole));
                assert_eq!(
                    divided.parts[2].cell(0, 2).unwrap(),
                    ValueRef::Str("SubjectKey-5")
                );
            }
        }

        /// The generated ordinal of each record is its position in the whole
        /// source, in every partition, from the CSV scan and from the
        /// Parquet row groups alike (INPUT-3).
        #[test]
        fn each_records_ordinal_is_its_position_in_the_whole_source() {
            let csv = declaration(SourceProfile::Csv, &[], Some("POSITION"));
            #[cfg_attr(not(feature = "parquet"), allow(unused_mut))]
            let mut inputs = vec![(csv, b"ID\nv\nw\nx\ny\nz\n".to_vec(), vec![2, 2, 1])];
            #[cfg(feature = "parquet")]
            {
                use ::parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
                use arrow_array::{ArrayRef, RecordBatch, StringArray};
                use arrow_schema::{DataType, Field as ArrowField, Schema};
                use std::sync::Arc;
                let schema = Arc::new(Schema::new(vec![ArrowField::new(
                    "ID",
                    DataType::Utf8,
                    true,
                )]));
                let values = Arc::new(StringArray::from(vec!["v", "w", "x", "y", "z"])) as ArrayRef;
                let batch = RecordBatch::try_new(schema.clone(), vec![values]).unwrap();
                let properties = WriterProperties::builder()
                    .set_max_row_group_size(2)
                    .build();
                let mut bytes = Vec::new();
                let mut writer =
                    ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                let parquet = declaration(SourceProfile::Parquet, &[], Some("POSITION"));
                inputs.push((parquet, bytes, vec![2, 2, 1]));
            }
            for (source, bytes, partitions) in inputs {
                let case = source.path.clone();
                let (whole, divided) = read(&source, &bytes, SourceInterpretation::Ordinary, 2);
                let (whole, divided) = (whole.unwrap(), divided.unwrap());
                assert_eq!(divided.sizes, partitions, "{case}");
                let positions = |table: &Snapshot<Infallible>| -> Vec<ValueRef<'static>> {
                    (0..table.row_count())
                        .map(|row| match table.cell(row, 1).unwrap() {
                            ValueRef::Int(value) => ValueRef::Int(value),
                            other => panic!("{case}: {other:?}"),
                        })
                        .collect()
                };
                let expected: Vec<ValueRef<'static>> = (1..=5).map(ValueRef::Int).collect();
                assert_eq!(positions(&whole), expected, "the whole read of {case}");
                let divided: Vec<ValueRef<'static>> =
                    divided.parts.iter().flat_map(positions).collect();
                assert_eq!(divided, expected, "partitions of {case}");
            }
        }

        /// The schema a CSV header or a Parquet footer declares is the
        /// decoded table's: declared types and the generated ordinal
        /// included. A header that does not read, and every other profile,
        /// gives none, so the input is read whole.
        #[test]
        fn the_header_or_footer_gives_the_decoded_schema() {
            let mut decoder = Decoder::<Infallible>::default();
            let typed = declaration(
                SourceProfile::Csv,
                &[("N", ColumnType::Int)],
                Some("POSITION"),
            );
            assert_eq!(
                decoder.partition_schema(
                    &typed,
                    b"ID,\"N\"\r\n1,\"x\ny\"\nnot read\"",
                    SourceInterpretation::Ordinary
                ),
                Some(schema(&[
                    ("ID", ColumnType::Str),
                    ("N", ColumnType::Int),
                    ("POSITION", ColumnType::Int)
                ]))
            );
            let undeclared = declaration(SourceProfile::Csv, &[], None);
            for (bytes, case) in [
                (&b""[..], "no header"),
                (b"A,A\n1,2\n", "a repeated name"),
                (b"A,\n1,2\n", "an empty name"),
                (b"A,\"B\n", "an unterminated quote"),
                (b"\xef\xbb\xbfA\n1\n", "an undeclared byte-order mark"),
                (b"A,\xff\n", "invalid text"),
            ] {
                assert_eq!(
                    decoder.partition_schema(&undeclared, bytes, SourceInterpretation::Ordinary),
                    None,
                    "{case}"
                );
            }
            for profile in [
                SourceProfile::Xport,
                SourceProfile::Sas7bdat,
                SourceProfile::OdmXml,
            ] {
                assert_eq!(
                    decoder.partition_schema(
                        &declaration(profile, &[], None),
                        b"ID\n1\n",
                        SourceInterpretation::Ordinary
                    ),
                    None,
                    "{profile:?}"
                );
            }
            #[cfg(feature = "parquet")]
            {
                use ::parquet::arrow::ArrowWriter;
                use arrow_array::{ArrayRef, Int64Array, RecordBatch};
                use arrow_schema::{DataType, Field as ArrowField, Schema};
                use std::sync::Arc;
                let schema_of = Arc::new(Schema::new(vec![ArrowField::new(
                    "ID",
                    DataType::Int64,
                    true,
                )]));
                let values = Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef;
                let batch = RecordBatch::try_new(schema_of.clone(), vec![values]).unwrap();
                let mut bytes = Vec::new();
                let mut writer = ArrowWriter::try_new(&mut bytes, schema_of, None).unwrap();
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                let parquet = declaration(SourceProfile::Parquet, &[], Some("POSITION"));
                assert_eq!(
                    decoder.partition_schema(&parquet, &bytes, SourceInterpretation::Ordinary),
                    Some(schema(&[
                        ("ID", ColumnType::Int),
                        ("POSITION", ColumnType::Int)
                    ]))
                );
                assert_eq!(
                    decoder.partition_schema(&parquet, b"PAR1", SourceInterpretation::Ordinary),
                    None
                );
            }
        }
    }
}
