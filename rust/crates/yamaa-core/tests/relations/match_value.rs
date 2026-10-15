use yamaa_core::{
    key_relation::ComparableType,
    match_value::{result_type, Expression, LiteralKind},
    reference_binding::{Catalog, Field},
    value::ColumnType,
};

/// Unknown names defer comparison without changing the immutable catalog's known types.
#[test]
fn unknown_results_do_not_poison_catalog() {
    let catalog = Catalog::compile(
        &[Field {
            name: "A",
            column_type: ColumnType::Int,
        }],
        &[],
    )
    .unwrap();
    assert_eq!(result_type(&catalog, Expression::Source("éé")), None);
    assert_eq!(result_type(&catalog, Expression::Source("xxx")), None);
    assert_eq!(
        result_type(&catalog, Expression::Source("A")),
        Some(ColumnType::Int.into())
    );
    assert_eq!(
        result_type(&catalog, Expression::Literal(LiteralKind::Bool)),
        Some(ComparableType::Boolean)
    );
    assert_eq!(
        result_type(&catalog, Expression::Literal(LiteralKind::Missing)),
        None
    );
}

#[test]
fn operations_outside_the_complete_req1259_table_defer_comparison() {
    let catalog = Catalog::compile(&[], &[]).unwrap();
    for name in [
        "date_shift",
        "datetime_shift",
        "datetime_diff",
        "duration_text",
        "duration_value",
        "row_value",
        "locf",
        "previous_non_missing",
        "mapping",
        "case",
        "greatest",
        "least",
    ] {
        assert_eq!(
            result_type(&catalog, Expression::Operation(name)),
            None,
            "{name}"
        );
    }
}
