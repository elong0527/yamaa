use yamaa_core::{
    csv_source::{self, CsvSource},
    table::{CellError, TableAccess, ValueRef},
    typed_csv::{Error, PreparedTypes},
    value::ColumnType,
};

#[test]
fn malformed_external_carriers_have_no_language_projection_or_index_panic() {
    for source in [
        CsvSource {
            names: vec![],
            records: vec![],
        },
        CsvSource {
            names: vec!["".into()],
            records: vec![vec![None]],
        },
        CsvSource {
            names: vec!["A".into(), "A".into()],
            records: vec![vec![None, None]],
        },
        CsvSource {
            names: vec!["A".into()],
            records: vec![vec![]],
        },
        CsvSource {
            names: vec!["A".into()],
            records: vec![vec![None, None]],
        },
    ] {
        let error = PreparedTypes::new(&[], 0)
            .unwrap()
            .convert(source)
            .unwrap_err();
        assert!(matches!(error, Error::InvalidSource));
        assert!(error.diagnostic("SRC").is_none());
    }
    for (declarations, maximum) in [
        (vec![("".into(), ColumnType::Int)], 1),
        (
            vec![
                ("A".into(), ColumnType::Int),
                ("A".into(), ColumnType::Date),
            ],
            2,
        ),
        (vec![("A".into(), ColumnType::Int)], 0),
    ] {
        let error = PreparedTypes::new(&declarations, maximum).err().unwrap();
        assert!(matches!(error, Error::InvalidDeclarations));
        assert!(error.diagnostic("SRC").is_none());
    }
}

#[test]
fn converted_core_values_outlive_all_input_authority_and_reject_invalid_cell_indexes() {
    let rows = {
        let declarations = vec![("I".into(), ColumnType::Int)];
        let bytes = b"I,T\n9007199254740993,\"retained\"\n,NA\n".to_vec();
        let source = csv_source::parse(&bytes, Default::default()).unwrap();
        PreparedTypes::new(&declarations, 2)
            .unwrap()
            .convert(source)
            .unwrap()
    };
    assert_eq!(rows.row_count(), 2);
    assert_eq!(rows.cell(0, 0).unwrap(), ValueRef::Int(9007199254740993));
    assert_eq!(rows.cell(0, 1).unwrap(), ValueRef::Str("retained"));
    assert_eq!(rows.cell(1, 0).unwrap(), ValueRef::Missing);
    assert_eq!(rows.cell(1, 1).unwrap(), ValueRef::Str("NA"));
    assert!(matches!(
        rows.cell(2, 0),
        Err(CellError::OutOfBounds { row: 2, column: 0 })
    ));
    assert!(matches!(
        rows.cell(0, 2),
        Err(CellError::OutOfBounds { row: 0, column: 2 })
    ));
}
