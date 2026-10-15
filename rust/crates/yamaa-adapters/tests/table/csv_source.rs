#[test]
fn untyped_arrow_snapshot_owns_exact_source_cells_and_limits() {
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
