use yamaa_adapters::csv_source::Limits;

#[test]
fn untyped_arrow_snapshot_owns_exact_source_cells_and_limits() {
    use yamaa_adapters::{
        arrow_table::{TableError, TableLimits},
        csv_source::{parse_text_table, TextTableError},
    };
    use yamaa_core::table::{TableAccess, ValueRef};
    let limits = TableLimits {
        max_rows: 10,
        max_columns: 4,
        max_batches: 1,
        max_cells: 40,
    };
    let mut input = b"A,B\n9007199254740993,\"\"\nx\0y,NA".to_vec();
    let table = parse_text_table(&input, Limits::default(), limits).unwrap();
    input.fill(0);
    drop(input);
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("9007199254740993"));
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Missing);
    assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("x\0y"));
    assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("NA"));
    assert!(table.cell(2, 0).is_err());
    assert_eq!(
        parse_text_table(b"A,B", Limits::default(), limits)
            .unwrap()
            .row_count(),
        0
    );
    assert!(matches!(
        parse_text_table(
            b"A\nx",
            Limits::default(),
            TableLimits {
                max_cells: 0,
                ..limits
            }
        ),
        Err(TextTableError::Table(TableError::Limit {
            resource: "cells",
            ..
        }))
    ));
}
