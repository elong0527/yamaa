//! Binary source truth is authored independently of the candidate parser.
use std::path::Path;
use yamaa_adapters::sas_source::{self, Error};
use yamaa_core::{
    specification::{SourceDeclaration, SourceProfile},
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sas")
            .join(name),
    )
    .unwrap()
}
fn declaration(name: &str) -> SourceDeclaration {
    SourceDeclaration {
        name: "SRC".into(),
        path: name.into(),
        types: vec![],
        profile: SourceProfile::from_path(name).unwrap(),
        encoding: Some("windows-1252".into()),
        empty_string_present: false,
        ordinal: None,
    }
}
#[test]
fn rle_and_rdc_compression_equal_independent_upstream_csv_truth() {
    let expected = yamaa_adapters::csv_source::parse(&bytes("compressed-truth.csv")).unwrap();
    for file in ["rle.sas7bdat", "rdc.sas7bdat"] {
        let table = sas_source::parse(&bytes(file), &declaration(file)).unwrap();
        assert_eq!(table.row_count(), expected.records.len());
        assert_eq!(table.schema().columns().len(), 100);
        for (column, field) in table.schema().columns().iter().enumerate() {
            assert_eq!(field.name, expected.names[column]);
            assert_eq!(
                field.kind,
                if column % 4 == 1 {
                    ColumnType::Str
                } else {
                    ColumnType::Float
                }
            );
            for (row, record) in expected.records.iter().enumerate() {
                let actual = table.cell(row, column).unwrap();
                match record[column].as_deref() {
                    None => assert_eq!(actual, ValueRef::Missing),
                    Some(text) if column % 4 == 1 => assert_eq!(actual, ValueRef::Str(text)),
                    Some(number) => assert_eq!(
                        actual,
                        ValueRef::Float(
                            yamaa_core::value::FiniteFloat::new(number.parse().unwrap()).unwrap()
                        )
                    ),
                }
            }
        }
    }
}
#[test]
fn malformed_compressed_controls_fail_closed() {
    // Independently identified first compressed-row command offsets from the
    // upstream pandas SAS corruption controls, retained in the source inventory.
    for (file, offset, byte) in [
        ("rle.sas7bdat", 0x10000 + 55229, 0x10),
        ("rdc.sas7bdat", 118170, 184),
    ] {
        let mut data = bytes(file);
        data[offset] = byte;
        assert!(
            sas_source::parse(&data, &declaration(file)).is_err(),
            "{file}"
        );
    }
}
#[test]
fn rdc_overlapping_back_reference_propagates_newly_emitted_bytes() {
    let file = "rdc-big-endian.sas7bdat";
    let mut data = bytes(file);
    assert_eq!(&data[120922..120924], &[0x8d, 0x00]);
    data[120922..120924].copy_from_slice(&[0x81, 0x00]);
    let table = sas_source::parse(&data, &declaration(file)).unwrap();
    let independent_original = 0.621f64.to_be_bytes();
    let mut repeated = [0; 8];
    repeated[..4].copy_from_slice(&independent_original[4..]);
    repeated[4..].copy_from_slice(&independent_original[4..]);
    assert_eq!(
        table.cell(0, 10).unwrap(),
        ValueRef::Float(yamaa_core::value::FiniteFloat::new(f64::from_be_bytes(repeated)).unwrap())
    );
}
#[test]
fn both_xport_versions_preserve_order_padding_unicode_and_default_float() {
    for (file, text) in [("v5.xpt", "TEXT"), ("v8.xpt", "LONG_TEXT_NAME")] {
        let mut source = declaration(file);
        let table = sas_source::parse(&bytes(file), &source).unwrap();
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.schema().columns()[0].kind, ColumnType::Float);
        assert_eq!(table.schema().columns()[1].name, text);
        assert_eq!(
            table.cell(0, 0).unwrap(),
            ValueRef::Float(yamaa_core::value::FiniteFloat::new(2.0).unwrap())
        );
        assert_eq!(
            table.cell(1, 0).unwrap(),
            ValueRef::Float(yamaa_core::value::FiniteFloat::new(1.0).unwrap())
        );
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str(" café"));
        assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Missing);
        assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Str("."));
        assert_eq!(table.cell(1, 2).unwrap(), ValueRef::Missing);
        // DATE9-like physical formats above do not infer the column type.
        assert_eq!(table.schema().columns()[3].kind, ColumnType::Float);
        source.empty_string_present = true;
        source.types = vec![
            ("ID".into(), ColumnType::Int),
            ("DAY".into(), ColumnType::Date),
            ("STAMP".into(), ColumnType::DateTime),
        ];
        let table = sas_source::parse(&bytes(file), &source).unwrap();
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(2));
        assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str(""));
        assert_eq!(
            table.cell(0, 3).unwrap(),
            ValueRef::Date("1960-01-01".parse().unwrap())
        );
        assert_eq!(
            table.cell(1, 3).unwrap(),
            ValueRef::Date("1970-01-01".parse().unwrap())
        );
        assert_eq!(
            table.cell(1, 4).unwrap(),
            ValueRef::DateTime("1959-12-31T23:59:59".parse().unwrap())
        );
    }
}
#[test]
fn sas_dataset_numeric_date_formats_never_infer_and_epoch_narrowing_is_explicit() {
    let mut source = declaration("datetime.sas7bdat");
    let table = sas_source::parse(&bytes("datetime.sas7bdat"), &source).unwrap();
    assert_eq!(table.schema().columns().len(), 5);
    assert!(table
        .schema()
        .columns()
        .iter()
        .all(|column| column.kind == ColumnType::Float));
    source.types = vec![
        ("VAR1".into(), ColumnType::DateTime),
        ("VAR2".into(), ColumnType::Date),
        ("VAR3".into(), ColumnType::Date),
        ("VAR4".into(), ColumnType::Date),
        ("VAR5".into(), ColumnType::Int),
    ];
    let table = sas_source::parse(&bytes("datetime.sas7bdat"), &source).unwrap();
    assert_eq!(
        table.cell(0, 0).unwrap(),
        ValueRef::DateTime("2015-02-02T14:42:12".parse().unwrap())
    );
    for column in [1, 2, 3] {
        assert_eq!(
            table.cell(0, column).unwrap(),
            ValueRef::Date("2015-02-02".parse().unwrap())
        );
    }
    assert_eq!(table.cell(0, 4).unwrap(), ValueRef::Int(52932));
}
#[test]
fn all_numeric_missing_tags_map_to_missing() {
    let table = sas_source::parse(&bytes("missing.xpt"), &declaration("missing.xpt")).unwrap();
    assert_eq!(table.row_count(), 28);
    for row in 0..28 {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
    }
}
#[test]
fn nonfinite_numeric_source_is_missing_before_every_authored_narrowing() {
    let mut payload = bytes("fractional.xpt");
    // Independent v5 layout: 640 header bytes, 160 NAMESTR/padding bytes,
    // 80 OBS bytes, then the first 8-byte numeric cell. ReadStat's documented
    // IBM non-finite sentinel maps to infinity, then core missing normalization.
    payload[880..888].copy_from_slice(&[0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    for kind in [
        ColumnType::Float,
        ColumnType::Int,
        ColumnType::Date,
        ColumnType::DateTime,
    ] {
        let mut source = declaration("fractional.xpt");
        source.types = vec![("NUM".into(), kind)];
        let table = sas_source::parse(&payload, &source).unwrap();
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
    }
}
#[test]
fn utf8_and_integer_boundaries_have_independent_exact_truth() {
    let mut source = declaration("utf8.xpt");
    source.encoding = Some("utf-8".into());
    let table = sas_source::parse(&bytes("utf8.xpt"), &source).unwrap();
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str(" café"));
    let mut source = declaration("integer-bounds.xpt");
    source.types = vec![("NUM".into(), ColumnType::Int)];
    let table = sas_source::parse(&bytes("integer-bounds.xpt"), &source).unwrap();
    for (row, number) in [i64::MIN, 0, 9_007_199_254_740_992, i64::MAX - 1023]
        .into_iter()
        .enumerate()
    {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(number));
    }
    source.path = "integer-overflow.xpt".into();
    assert!(matches!(
        sas_source::parse(&bytes("integer-overflow.xpt"), &source,),
        Err(Error::Typing(
            yamaa_core::typed_csv::Error::FieldParse { .. }
        ))
    ));
}
#[test]
fn unsupported_compression_has_a_specific_retained_cause() {
    let source = declaration("unsupported-compression.sas7bdat");
    assert!(matches!(
        sas_source::parse(&bytes(&source.path), &source),
        Err(Error::Invalid("compression"))
    ));
}
#[test]
fn reject_multiple_members_in_both_transport_versions() {
    for name in ["multi-v5.xpt", "multi-v8.xpt"] {
        assert!(matches!(
            sas_source::parse(&bytes(name), &declaration(name)),
            Err(Error::Invalid("members"))
        ));
    }
}
#[test]
fn member_header_shaped_character_data_is_an_ordinary_value() {
    let source = declaration("member-text.xpt");
    let table = sas_source::parse(&bytes(&source.path), &source).unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(
        table.cell(0, 0).unwrap(),
        ValueRef::Str(
            "HEADER RECORD*******MEMBER  HEADER RECORD!!!!!!!000000000000000000000000000000"
        )
    );
}
#[test]
fn ordinary_float_keeps_readstat_conversion_while_exact_narrowing_is_guarded() {
    for (file, expected) in [
        ("decimal.xpt", 0.1),
        ("precision.xpt", 9_007_199_254_740_992.0),
    ] {
        let source = declaration(file);
        let table = sas_source::parse(&bytes(file), &source).unwrap();
        assert_eq!(
            table.cell(0, 0).unwrap(),
            ValueRef::Float(yamaa_core::value::FiniteFloat::new(expected).unwrap())
        );
    }
}
#[test]
fn narrowing_never_truncates_or_accepts_an_ibm_precision_loss() {
    for name in ["fractional.xpt", "precision.xpt"] {
        for kind in [ColumnType::Int, ColumnType::Date, ColumnType::DateTime] {
            let mut source = declaration(name);
            source.types = vec![("NUM".into(), kind)];
            assert!(matches!(
                sas_source::parse(&bytes(name), &source),
                Err(Error::Typing(
                    yamaa_core::typed_csv::Error::FieldParse { .. }
                ))
            ));
        }
    }
}
#[test]
fn malformed_truncated_encoding_and_unknown_fields_fail_before_execution() {
    let source = declaration("v5.xpt");
    let payload = bytes("v5.xpt");
    for length in [0, 1, 79, 640, payload.len() - 1] {
        assert!(sas_source::parse(&payload[..length], &source).is_err());
    }
    let mut source = source;
    source.encoding = Some("utf-8".into());
    assert!(matches!(
        sas_source::parse(&payload, &source),
        Err(Error::Invalid("encoding"))
    ));
    source.encoding = Some("windows-1252".into());
    source.types = vec![("ABSENT".into(), ColumnType::Int)];
    assert!(matches!(
        sas_source::parse(&payload, &source),
        Err(Error::Typing(
            yamaa_core::typed_csv::Error::UnknownField { .. }
        ))
    ));
    let sas = bytes("datetime.sas7bdat");
    for length in [0, 31, 1024, sas.len() - 1] {
        assert!(matches!(
            sas_source::parse(&sas[..length], &declaration("datetime.sas7bdat"),),
            Err(Error::Invalid(_))
        ));
    }
}
