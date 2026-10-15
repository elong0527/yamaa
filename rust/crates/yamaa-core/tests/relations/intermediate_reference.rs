use yamaa_core::{
    intermediate_reference::{
        validate_field, validate_read, Finding as F, Intermediate, Read, Source,
    },
    reference_binding::{Catalog, Dataset, Field},
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
            validate_field(&catalog, field, target),
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
        assert_eq!(validate_read(&catalog, read), expected, "{read:?}");
    }
}

/// Self-reads stay deferred to cycle analysis, and Unicode visibility has no shared state.
#[test]
fn self_reads_and_unicode_visibility_are_reusable() {
    let catalog = catalog();
    let target = Intermediate {
        source: Source::Dataset("SRC"),
        derived: &[],
        readable: &[],
        dependencies: &[],
    };
    assert_eq!(validate_field(&catalog, "é", target), None);
    let read = Read {
        reader: "same",
        target_name: "same",
        target: None,
        field: "K",
        donor_dataset: "D",
        visible: &["é"],
    };
    assert_eq!(validate_read(&catalog, read), vec![]);
    assert_eq!(validate_field(&catalog, "é", target), None);
}
