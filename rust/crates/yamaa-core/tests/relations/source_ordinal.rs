use yamaa_core::{
    source_ordinal::{stored_fields, Error, Ordinal},
    table::{Column, TableSchema},
    value::ColumnType,
};

fn schema() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Str,
    }])
    .unwrap()
}

#[test]
fn positions_and_stored_contract_are_exact_at_the_admission_boundary() {
    let original = schema();
    let admitted = Ordinal::admit(&original, 3, "REC").unwrap();
    assert_eq!(
        stored_fields(admitted.schema(), Some("REC")).unwrap(),
        original.columns()
    );
    assert_eq!(
        admitted.schema().columns()[1],
        Column {
            name: "REC".into(),
            kind: ColumnType::Int
        }
    );
    assert_eq!(
        (
            admitted.value(0),
            admitted.value(2),
            admitted.value(3),
            admitted.value(usize::MAX)
        ),
        (Some(1), Some(3), None, None)
    );

    assert!(matches!(
        Ordinal::admit(&original, usize::MAX, "REC"),
        Err(Error::ResourceLimit { .. })
    ));
    assert!(matches!(
        Ordinal::admit(&original, 0, "ID"),
        Err(Error::Collision)
    ));
    assert!(matches!(
        stored_fields(&original, Some("REC")),
        Err(Error::InvalidTable)
    ));
    assert!(matches!(
        stored_fields(&original, Some("ID")),
        Err(Error::InvalidTable)
    ));
    let empty = Ordinal::admit(&original, 0, "REC").unwrap();
    assert_eq!(empty.schema(), admitted.schema());
    assert_eq!(empty.value(0), None);
    assert_eq!(stored_fields(&original, None).unwrap(), original.columns());
    let edge = Ordinal::admit(&original, 10_000_000, "REC").unwrap();
    assert_eq!(edge.value(9_999_999), Some(10_000_000));
    assert!(matches!(
        Ordinal::admit(&original, 10_000_001, "REC"),
        Err(Error::ResourceLimit {
            resource: "table_rows",
            limit: 10_000_000,
            actual: Some(10_000_001)
        })
    ));
    let wide = |count| {
        TableSchema::new(
            (0..count)
                .map(|index| Column {
                    name: format!("F{index}"),
                    kind: ColumnType::Str,
                })
                .collect(),
        )
        .unwrap()
    };
    let admitted = Ordinal::admit(&wide(1023), 625_000, "REC").unwrap();
    assert_eq!(admitted.schema().columns().len(), 1024);
    assert!(matches!(
        Ordinal::admit(&wide(1024), 0, "REC"),
        Err(Error::ResourceLimit {
            resource: "columns",
            limit: 1024,
            actual: Some(1025)
        })
    ));
    assert!(matches!(
        Ordinal::admit(&wide(1023), 625_001, "REC"),
        Err(Error::ResourceLimit {
            resource: "table_cells",
            limit: 640_000_000,
            actual: Some(640_001_024)
        })
    ));
}
