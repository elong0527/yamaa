use yamaa_core::{
    reference_binding::{Catalog, Dataset, Error, Field},
    reference_scope::{validate, Finding as F, Phase, Reach, Scope},
    value::ColumnType::{Float, Int, Str},
};

/// Independent typed source metadata; no reference implementation generates expected findings.
fn catalog() -> Catalog {
    let source = [
        Field {
            name: "K",
            column_type: Str,
        },
        Field {
            name: "N",
            column_type: Int,
        },
    ];
    let other = [Field {
        name: "N",
        column_type: Float,
    }];
    Catalog::compile(
        &[],
        &[
            Dataset {
                name: "SRC",
                fields: &source,
            },
            Dataset {
                name: "OTHER",
                fields: &other,
            },
        ],
    )
    .unwrap()
}

/// A direct scalar read in the selected row or column phase.
fn scope(phase: Phase<'_>) -> Scope<'_> {
    Scope {
        drivers: &["SRC"],
        current_driver: false,
        reach: Reach::Scalar,
        joined: false,
        phase,
    }
}

/// Stop after driver/existence failures, but retain scope-before-type diagnostic order.
#[test]
fn independent_scope_and_priority_truth() {
    let catalog = catalog();
    let row = scope(Phase::Row { group_by: None });
    let group = scope(Phase::Row {
        group_by: Some(&["SRC.K"]),
    });
    let column = scope(Phase::Column { groups: &[] });
    let columns = scope(Phase::Column {
        groups: &[&["SRC.K", "SRC.N"], &["SRC.K"]],
    });
    for (name, expected, scope, findings) in [
        ("SRC.N", Some(Int), row, vec![]),
        (
            "SRC.N",
            Some(Str),
            row,
            vec![F::IncompatibleInputType {
                expected: Str,
                actual: Int,
            }],
        ),
        ("SRC.K", Some(Str), group, vec![]),
        ("SRC.N", None, group, vec![F::RowGroup]),
        (
            "SRC.N",
            Some(Str),
            group,
            vec![
                F::RowGroup,
                F::IncompatibleInputType {
                    expected: Str,
                    actual: Int,
                },
            ],
        ),
        ("SRC.ABSENT", Some(Str), group, vec![F::UnknownField]),
        (
            "OTHER.ABSENT",
            Some(Str),
            Scope {
                current_driver: true,
                ..group
            },
            vec![F::DriverMismatch],
        ),
        (
            "SRC.ABSENT",
            None,
            Scope {
                current_driver: true,
                ..row
            },
            vec![F::UnknownField],
        ),
        (
            "SRC.N",
            None,
            Scope {
                current_driver: true,
                drivers: &[],
                ..row
            },
            vec![F::DriverMismatch],
        ),
        (
            "SRC.N",
            None,
            Scope {
                current_driver: true,
                drivers: &["SRC", "OTHER"],
                ..row
            },
            vec![F::DriverMismatch],
        ),
        (
            "SRC.N",
            None,
            Scope {
                current_driver: true,
                drivers: &["SRC", "SRC"],
                ..row
            },
            vec![],
        ),
        (
            "SRC.N",
            None,
            Scope {
                joined: true,
                ..group
            },
            vec![],
        ),
        (
            "SRC.N",
            None,
            Scope {
                reach: Reach::Record,
                ..group
            },
            vec![],
        ),
        (
            "SRC.N",
            None,
            Scope {
                reach: Reach::Relation,
                ..group
            },
            vec![],
        ),
        (
            "SRC.N",
            None,
            Scope {
                reach: Reach::Declared,
                ..group
            },
            vec![],
        ),
        (
            "OTHER.N",
            Some(Str),
            group,
            vec![F::IncompatibleInputType {
                expected: Str,
                actual: Float,
            }],
        ),
        (
            "OTHER.N",
            Some(Str),
            Scope {
                reach: Reach::Relation,
                ..group
            },
            vec![
                F::RowPhase,
                F::IncompatibleInputType {
                    expected: Str,
                    actual: Float,
                },
            ],
        ),
        (
            "OTHER.N",
            None,
            Scope {
                reach: Reach::Declared,
                ..group
            },
            vec![],
        ),
        ("SRC.N", None, columns, vec![F::ColumnGroup]),
        (
            "SRC.N",
            Some(Str),
            columns,
            vec![
                F::ColumnGroup,
                F::IncompatibleInputType {
                    expected: Str,
                    actual: Int,
                },
            ],
        ),
        ("SRC.K", Some(Str), columns, vec![]),
        (
            "SRC.N",
            None,
            Scope {
                reach: Reach::Relation,
                ..columns
            },
            vec![],
        ),
        ("OTHER.N", Some(Str), column, vec![]),
        ("OTHER.ABSENT", Some(Str), column, vec![F::UnknownField]),
        (
            "OTHER.N",
            Some(Str),
            Scope {
                joined: true,
                ..column
            },
            vec![F::IncompatibleInputType {
                expected: Str,
                actual: Float,
            }],
        ),
        (
            "OTHER.N",
            Some(Str),
            Scope {
                reach: Reach::Declared,
                ..column
            },
            vec![F::IncompatibleInputType {
                expected: Str,
                actual: Float,
            }],
        ),
        (
            "OTHER.N",
            Some(Str),
            Scope {
                reach: Reach::Relation,
                ..column
            },
            vec![F::IncompatibleInputType {
                expected: Str,
                actual: Float,
            }],
        ),
        (
            "SRC.K",
            None,
            scope(Phase::Row {
                group_by: Some(&[]),
            }),
            vec![F::RowGroup],
        ),
        (
            "SRC.K",
            None,
            scope(Phase::Column { groups: &[&[]] }),
            vec![F::ColumnGroup],
        ),
    ] {
        assert_eq!(
            validate(&catalog, name, expected, scope),
            Ok(findings),
            "{name}: {scope:?}"
        );
    }
}

/// A malformed query remains separate from later valid scope checks.
#[test]
fn qualified_query_shape_and_reuse() {
    let catalog = catalog();
    let row = scope(Phase::Row { group_by: None });
    assert_eq!(
        validate(&catalog, "N", None, row),
        Err(Error::BareQualifiedQuery)
    );
    assert_eq!(validate(&catalog, "SRC.N", Some(Int), row), Ok(vec![]));
}
