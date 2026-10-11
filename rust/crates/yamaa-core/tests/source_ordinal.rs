use yamaa_core::{
    source_ordinal::{stored_fields, Error, Limits, Ordinal},
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
    let admitted = Ordinal::admit(
        &original,
        3,
        "REC",
        Limits {
            columns: 2,
            cells: 6,
        },
    )
    .unwrap();
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
    for limits in [
        Limits {
            columns: 1,
            cells: 6,
        },
        Limits {
            columns: 2,
            cells: 5,
        },
    ] {
        assert!(matches!(
            Ordinal::admit(&original, 3, "REC", limits),
            Err(Error::Limit)
        ));
    }
    assert!(matches!(
        Ordinal::admit(
            &original,
            usize::MAX,
            "REC",
            Limits {
                columns: usize::MAX,
                cells: usize::MAX
            }
        ),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Ordinal::admit(
            &original,
            0,
            "ID",
            Limits {
                columns: 0,
                cells: 0
            }
        ),
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
    let empty = Ordinal::admit(
        &original,
        0,
        "REC",
        Limits {
            columns: 2,
            cells: 0,
        },
    )
    .unwrap();
    assert_eq!(empty.schema(), admitted.schema());
    assert_eq!(empty.value(0), None);
    assert_eq!(stored_fields(&original, None).unwrap(), original.columns());
}
