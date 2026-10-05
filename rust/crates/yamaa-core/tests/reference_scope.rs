use yamaa_core::{
    reference_binding::{Catalog, Dataset, Error, Field, Limits as CatalogLimits},
    reference_scope::{validate, Finding as F, Limits, Phase, Reach, Scope},
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
        CatalogLimits::default(),
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
            validate(&catalog, name, expected, scope, Limits::default()),
            Ok(findings),
            "{name}: {scope:?}"
        );
    }
}

/// Count written scope entries and UTF-8 bytes before language diagnostics, without state leakage.
#[test]
fn scope_admission_and_reuse() {
    let catalog = catalog();
    let row = scope(Phase::Row { group_by: None });
    assert_eq!(
        validate(&catalog, "N", None, row, Limits::default()),
        Err(Error::BareQualifiedQuery)
    );
    for (scope, limits, resource, limit, required) in [
        (
            Scope {
                drivers: &["SRC", "SRC"],
                ..row
            },
            Limits {
                drivers: 1,
                ..Limits::default()
            },
            "scope_drivers",
            1,
            2,
        ),
        (
            scope(Phase::Column {
                groups: &[&[], &[]],
            }),
            Limits {
                groups: 1,
                ..Limits::default()
            },
            "scope_groups",
            1,
            2,
        ),
        (
            scope(Phase::Row {
                group_by: Some(&["SRC.K", "SRC.K"]),
            }),
            Limits {
                group_fields: 1,
                ..Limits::default()
            },
            "scope_group_fields",
            1,
            2,
        ),
        (
            Scope {
                drivers: &["é"],
                ..row
            },
            Limits {
                context_bytes: 1,
                ..Limits::default()
            },
            "scope_context_bytes",
            1,
            2,
        ),
    ] {
        assert_eq!(
            validate(&catalog, "UNKNOWN.N", None, scope, limits),
            Err(Error::Limit {
                resource,
                limit,
                required
            })
        );
    }
    assert_eq!(
        validate(&catalog, "SRC.N", Some(Int), row, Limits::default()),
        Ok(vec![])
    );
}
