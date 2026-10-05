use yamaa_core::{
    key_relation::ComparableType,
    match_value::{result_type, Expression, LiteralKind, MAX_OPERATION_BYTES},
    reference_binding::{Catalog, Error, Field, Limits},
    value::ColumnType,
};

/// Unknown operation names cannot bypass UTF-8 resource admission; catalogs retain their own policy.
#[test]
fn limits_precede_unknown_results_and_do_not_poison_catalog() {
    let catalog = Catalog::compile(
        &[Field {
            name: "A",
            column_type: ColumnType::Int,
        }],
        &[],
        Limits {
            reference_bytes: 3,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(
        result_type(&catalog, Expression::Source("éé")),
        Err(Error::Limit {
            resource: "reference_bytes",
            limit: 3,
            required: 4,
        })
    );
    assert_eq!(result_type(&catalog, Expression::Source("xxx")), Ok(None));
    assert_eq!(
        result_type(
            &catalog,
            Expression::Operation(&"é".repeat(MAX_OPERATION_BYTES / 2))
        ),
        Ok(None)
    );
    assert_eq!(
        result_type(
            &catalog,
            Expression::Operation(&"é".repeat(MAX_OPERATION_BYTES / 2 + 1))
        ),
        Err(Error::Limit {
            resource: "operation_bytes",
            limit: MAX_OPERATION_BYTES,
            required: MAX_OPERATION_BYTES + 2,
        })
    );
    assert_eq!(
        result_type(&catalog, Expression::Source("A")),
        Ok(Some(ColumnType::Int.into()))
    );
    assert_eq!(
        result_type(&catalog, Expression::Literal(LiteralKind::Bool)),
        Ok(Some(ComparableType::Boolean))
    );
    assert_eq!(
        result_type(&catalog, Expression::Literal(LiteralKind::Missing)),
        Ok(None)
    );
}
