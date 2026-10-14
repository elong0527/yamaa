use yamaa_core::{
    intermediate_reference::{
        validate_field, validate_read, Finding as F, Intermediate, Limits, Read, Source,
    },
    reference_binding::{Catalog, Dataset, Error, Field, Limits as CatalogLimits},
    value::ColumnType,
};

/// Authored source metadata includes a literal dotted field and a Unicode field.
fn catalog() -> Catalog {
    Catalog::compile(
        &[],
        &[Dataset {
            name: "SRC",
            fields: &[
                Field {
                    name: "K",
                    column_type: ColumnType::Str,
                },
                Field {
                    name: "N",
                    column_type: ColumnType::Int,
                },
                Field {
                    name: "X.Y",
                    column_type: ColumnType::Str,
                },
                Field {
                    name: "é",
                    column_type: ColumnType::Str,
                },
            ],
        }],
        CatalogLimits::default(),
    )
    .unwrap()
}

/// A planned lookup adds one field while otherwise exposing its stored columns.
fn target() -> Intermediate<'static> {
    Intermediate {
        source: Source::Dataset("SRC"),
        derived: &["D"],
        readable: &[],
        dependencies: &[],
    }
}

/// REQ-0125 expectations come from authored visibility, not generated runtime results.
#[test]
fn stored_derived_and_self_visibility() {
    let catalog = catalog();
    let all = target();
    let selected = Intermediate {
        readable: &["K", "D", "ABSENT"],
        ..all
    };
    let own = Intermediate {
        source: Source::SelfFields(&["K"]),
        ..all
    };
    let absent = Intermediate {
        source: Source::Dataset("ABSENT"),
        ..all
    };
    for (field, target, finding) in [
        ("K", all, None),
        ("D", all, None),
        ("N", all, None),
        ("X.Y", all, None),
        ("é", all, None),
        ("É", all, Some(F::UnknownField)),
        ("ABSENT", all, Some(F::UnknownField)),
        ("K", selected, None),
        ("D", selected, None),
        ("N", selected, Some(F::UnknownField)),
        ("ABSENT", selected, Some(F::UnknownField)),
        ("K", own, None),
        ("D", own, None),
        ("N", own, Some(F::UnknownField)),
        (
            "K",
            Intermediate {
                readable: &["D"],
                ..own
            },
            Some(F::UnknownField),
        ),
        ("ABSENT", absent, None),
        (
            "N",
            Intermediate {
                derived: &["N"],
                ..all
            },
            None,
        ),
    ] {
        assert_eq!(
            validate_field(&catalog, field, target, Limits::default()).unwrap(),
            finding,
            "{field}, {target:?}"
        );
    }
}

/// Donor records expose stored and earlier-derived names; dependencies keep declaration order.
#[test]
fn donor_scope_phase_and_priority() {
    let catalog = catalog();
    let target = Intermediate {
        dependencies: &[
            "K",
            "DONOR.K",
            "DONOR.X.Y",
            "DONOR.D",
            "OTHER.K",
            "LATER",
            "OTHER.K",
        ],
        ..target()
    };
    let read = Read {
        reader: "reader",
        target_name: "lookup",
        target: Some(target),
        field: "K",
        donor_dataset: "DONOR",
        visible: &["K", "X.Y", "D"],
    };
    let missing = vec![
        F::UnavailableDependency { dependency: 4 },
        F::UnavailableDependency { dependency: 5 },
        F::UnavailableDependency { dependency: 6 },
    ];
    for (read, expected) in [
        (read, missing.clone()),
        (
            Read {
                field: "ABSENT",
                ..read
            },
            [vec![F::UnknownField], missing.clone()].concat(),
        ),
        (
            Read {
                target: None,
                ..read
            },
            vec![],
        ),
        (
            Read {
                reader: "lookup",
                ..read
            },
            vec![],
        ),
        (
            Read {
                target: Some(Intermediate {
                    source: Source::SelfFields(&["K"]),
                    ..target
                }),
                ..read
            },
            vec![F::SelfPhase],
        ),
        (
            Read {
                reader: "lookup",
                target: Some(Intermediate {
                    source: Source::SelfFields(&[]),
                    ..target
                }),
                ..read
            },
            vec![],
        ),
        (
            Read {
                target: Some(Intermediate {
                    dependencies: &[],
                    ..target
                }),
                field: "D",
                ..read
            },
            vec![],
        ),
        (
            Read {
                target: Some(Intermediate {
                    source: Source::Dataset("ABSENT"),
                    dependencies: &[],
                    ..target
                }),
                field: "D",
                ..read
            },
            vec![],
        ),
        (
            Read {
                target: Some(Intermediate {
                    source: Source::Dataset("ABSENT"),
                    dependencies: &[],
                    ..target
                }),
                ..read
            },
            vec![F::UnknownField],
        ),
        (
            Read {
                target: Some(Intermediate {
                    readable: &["D"],
                    dependencies: &[],
                    ..target
                }),
                ..read
            },
            vec![F::UnknownField],
        ),
        (
            Read {
                target: Some(Intermediate {
                    dependencies: &["DONOR.X.Y"],
                    ..target
                }),
                visible: &["X", "Y"],
                ..read
            },
            vec![F::UnavailableDependency { dependency: 0 }],
        ),
        (
            Read {
                target: Some(Intermediate {
                    dependencies: &["é", "DONOR.é"],
                    ..target
                }),
                visible: &["é", "é"],
                ..read
            },
            vec![],
        ),
    ] {
        assert_eq!(
            validate_read(&catalog, read, Limits::default()).unwrap(),
            expected,
            "{read:?}"
        );
    }
}

/// Written duplicates and UTF-8 bytes count before language short-circuits; rejection is reusable.
#[test]
fn resource_admission_before_skipped_reads() {
    let catalog = catalog();
    let target = Intermediate {
        source: Source::Dataset("SRC"),
        derived: &[],
        readable: &[],
        dependencies: &[],
    };
    let limits = Limits {
        entries: 2,
        context_bytes: 5,
    };
    assert_eq!(validate_field(&catalog, "é", target, limits).unwrap(), None);
    assert_eq!(
        validate_field(&catalog, "éé", target, limits),
        Err(Error::Limit {
            resource: "intermediate_context_bytes",
            limit: 5,
            required: 7
        })
    );
    assert_eq!(
        validate_field(
            &catalog,
            "K",
            Intermediate {
                derived: &["K", "K"],
                ..target
            },
            limits
        ),
        Err(Error::Limit {
            resource: "intermediate_entries",
            limit: 2,
            required: 4
        })
    );
    let read = Read {
        reader: "same",
        target_name: "same",
        target: None,
        field: "K",
        donor_dataset: "D",
        visible: &["é"],
    };
    assert_eq!(
        validate_read(
            &catalog,
            read,
            Limits {
                entries: 4,
                ..Limits::default()
            }
        ),
        Err(Error::Limit {
            resource: "intermediate_entries",
            limit: 4,
            required: 5
        })
    );
    assert_eq!(
        validate_read(
            &catalog,
            read,
            Limits {
                context_bytes: 11,
                ..Limits::default()
            }
        ),
        Err(Error::Limit {
            resource: "intermediate_context_bytes",
            limit: 11,
            required: 12
        })
    );
    assert_eq!(
        validate_read(&catalog, read, Limits::default()).unwrap(),
        vec![]
    );
    assert_eq!(validate_field(&catalog, "é", target, limits).unwrap(), None);
}
