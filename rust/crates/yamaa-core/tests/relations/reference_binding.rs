use yamaa_core::reference_binding::{Binding, Catalog, Dataset, Diagnostic, Error, Field};
use yamaa_core::value::ColumnType::{self, Date, Float, Int, Str};

/// Named typed declarations, independent of records or any host table representation.
fn field(name: &str, column_type: ColumnType) -> Field<'_> {
    Field { name, column_type }
}

/// Copy metadata from short-lived buffers so every test also exercises catalog ownership.
fn catalog() -> Catalog {
    let outputs = [
        field("OUT", Str),
        field("X", Int),
        field("WHEN", Date),
        field("β", Float),
    ];
    let z = [
        field("X", Int),
        field("F", Str),
        field("IT.X", Str),
        field("é", Str),
    ];
    let a = [field("X", Float), field("A", Date), field("F", Int)];
    let alpha = [field("X", Str)];
    let datasets = [
        Dataset {
            name: "Z",
            fields: &z,
        },
        Dataset {
            name: "A",
            fields: &a,
        },
        Dataset {
            name: "α",
            fields: &alpha,
        },
    ];
    Catalog::compile(&outputs, &datasets).unwrap()
}

/// Dataset spelling is exact; bare outputs win without implicit source or ODM resolution.
#[test]
fn exact_names_and_declared_types() {
    let catalog = catalog();
    for (name, expected) in [
        (
            "OUT",
            Some(Binding::Output {
                column: 0,
                column_type: Str,
            }),
        ),
        (
            "X",
            Some(Binding::Output {
                column: 1,
                column_type: Int,
            }),
        ),
        (
            "WHEN",
            Some(Binding::Output {
                column: 2,
                column_type: Date,
            }),
        ),
        (
            "β",
            Some(Binding::Output {
                column: 3,
                column_type: Float,
            }),
        ),
        (
            "Z.X",
            Some(Binding::Dataset {
                dataset: 0,
                field: 0,
                column_type: Int,
            }),
        ),
        (
            "A.X",
            Some(Binding::Dataset {
                dataset: 1,
                field: 0,
                column_type: Float,
            }),
        ),
        (
            "Z.IT.X",
            Some(Binding::Dataset {
                dataset: 0,
                field: 2,
                column_type: Str,
            }),
        ),
        (
            "Z.é",
            Some(Binding::Dataset {
                dataset: 0,
                field: 3,
                column_type: Str,
            }),
        ),
        (
            "α.X",
            Some(Binding::Dataset {
                dataset: 2,
                field: 0,
                column_type: Str,
            }),
        ),
        ("F", None),
        ("OUT.X", None),
        ("DOMAIN.OUT", None),
        ("Z.IT.Y", None),
        ("Z.e\u{301}", None),
        ("z.X", None),
        ("Z.x", None),
        ("Z.", None),
        ("", None),
        (".X", None),
    ] {
        assert_eq!(catalog.bind(name), expected, "{name}");
    }
}

/// Unknown names precede phase failures, which precede strict expected-type checks.
#[test]
fn output_conditions_and_suggestion_order() {
    let catalog = catalog();
    for (name, expected, available, candidates, diagnostic) in [
        ("OUT", Some(Str), None, vec![], None),
        (
            "X",
            Some(Float),
            None,
            vec![],
            Some(Diagnostic::IncompatibleInputType {
                expected: Float,
                actual: Int,
            }),
        ),
        (
            "X",
            Some(Float),
            Some(vec![0]),
            vec![],
            Some(Diagnostic::PhaseBoundary { column: 1 }),
        ),
        ("X", Some(Int), Some(vec![1]), vec![], None),
        (
            "UNKNOWN",
            Some(Int),
            Some(vec![]),
            vec![2, 0, 1],
            Some(Diagnostic::UnknownField),
        ),
        (
            "F",
            Some(Int),
            Some(vec![]),
            vec![0, 1],
            Some(Diagnostic::UnresolvableName { dataset: 1 }),
        ),
        (
            "F",
            None,
            None,
            vec![0],
            Some(Diagnostic::UnresolvableName { dataset: 0 }),
        ),
        ("F", None, None, vec![2], Some(Diagnostic::UnknownField)),
        ("F", None, None, vec![], Some(Diagnostic::UnknownField)),
        ("β", Some(Float), Some(vec![3]), vec![], None),
    ] {
        assert_eq!(
            catalog.validate_output(name, expected, available.as_deref(), &candidates),
            Ok(diagnostic),
            "{name}"
        );
    }
}

/// Large candidate sets keep lexical selection, duplicate admission and per-query isolation.
#[test]
fn large_candidate_membership_preserves_suggestions() {
    let names: Vec<_> = (0..256).rev().map(|index| format!("D{index:03}")).collect();
    let fields = [field("F", Str)];
    let datasets: Vec<_> = names
        .iter()
        .map(|name| Dataset {
            name,
            fields: &fields,
        })
        .collect();
    let catalog = Catalog::compile(&[], &datasets).unwrap();
    let candidates: Vec<_> = (0..256).collect();
    assert_eq!(
        catalog.validate_output("ABSENT", None, None, &candidates),
        Ok(Some(Diagnostic::UnknownField))
    );
    assert_eq!(
        catalog.validate_output("F", None, None, &candidates),
        Ok(Some(Diagnostic::UnresolvableName { dataset: 255 }))
    );
    assert_eq!(
        catalog.validate_output("F", None, None, &[0; 256]),
        Ok(Some(Diagnostic::UnresolvableName { dataset: 0 }))
    );
    assert_eq!(
        catalog.validate_output("F", None, None, &[]),
        Ok(Some(Diagnostic::UnknownField))
    );
    assert_eq!(
        catalog.validate_output("F", None, None, &[0; 257]),
        Ok(Some(Diagnostic::UnresolvableName { dataset: 0 }))
    );
}

/// Invalid metadata keeps its structural cause and never poisons later catalogs.
#[test]
fn catalog_shape_and_reuse() {
    let outputs = [field("X", Int), field("X", Int)];
    assert!(matches!(
        Catalog::compile(&outputs, &[]),
        Err(Error::DuplicateOutput)
    ));
    assert!(matches!(
        Catalog::compile(&[field("", Int)], &[]),
        Err(Error::EmptyName)
    ));
    let duplicates = [
        Dataset {
            name: "D",
            fields: &[],
        },
        Dataset {
            name: "D",
            fields: &[],
        },
    ];
    assert!(matches!(
        Catalog::compile(&[], &duplicates),
        Err(Error::DuplicateDataset)
    ));
    assert!(matches!(
        Catalog::compile(
            &[],
            &[Dataset {
                name: "D",
                fields: &outputs
            }]
        ),
        Err(Error::DuplicateField)
    ));
    let catalog = catalog();
    assert_eq!(
        catalog.bind("X"),
        Some(Binding::Output {
            column: 1,
            column_type: Int
        })
    );
}

/// Bad context indices are metadata errors and never turn into a plausible name failure.
#[test]
fn query_admission_is_separate_and_stateless() {
    let catalog = catalog();
    for _ in 0..2 {
        assert_eq!(
            catalog.validate_output("F", None, Some(&[4]), &[]),
            Err(Error::InvalidAvailableOutput)
        );
        assert_eq!(
            catalog.validate_output("F", None, None, &[3]),
            Err(Error::InvalidCandidateDataset)
        );
        assert_eq!(
            catalog.validate_output("Z.X", None, None, &[]),
            Err(Error::QualifiedOutputQuery)
        );
        assert_eq!(
            catalog.validate_output("OUT", None, Some(&[]), &[]),
            Ok(Some(Diagnostic::PhaseBoundary { column: 0 }))
        );
        assert_eq!(
            catalog.validate_output("OUT", None, Some(&[0]), &[]),
            Ok(None)
        );
    }
    let unicode = Catalog::compile(&[field("é", Str)], &[]).unwrap();
    assert_eq!(
        unicode.bind("é"),
        Some(Binding::Output {
            column: 0,
            column_type: Str,
        })
    );
    assert_eq!(unicode.validate_output("é", None, None, &[]), Ok(None));
}

/// Whole-relation binding preserves declaration indices, including empty schemas and exact Unicode names.
#[test]
fn relation_identity_is_not_a_stored_field() {
    let catalog = Catalog::compile(
        &[field("OUT", Str)],
        &[
            Dataset {
                name: "EMPTY",
                fields: &[],
            },
            Dataset {
                name: "A.B",
                fields: &[],
            },
        ],
    )
    .unwrap();
    assert_eq!(catalog.bind_relation("EMPTY"), Some(0));
    assert_eq!(catalog.bind_relation("A.B"), Some(1));
    assert_eq!(catalog.bind_relation("OUT"), None);
    assert_eq!(catalog.bind_relation(""), None);
    assert_eq!(catalog.bind("EMPTY"), None);
    assert_eq!(catalog.bind_relation("\u{e9}\u{e9}\u{e9}"), None);
    assert_eq!(catalog.bind_relation("EMPTY"), Some(0));
}
