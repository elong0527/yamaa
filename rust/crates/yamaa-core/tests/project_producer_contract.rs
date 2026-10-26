//! Project contracts: producer contract.
//!
//! The seam is the `Contract` that `prepare_document` returns: its
//! fields, `matches_header`, `matches_typed_fields` and typed `Error`, which
//! no run reports, and its 1,024-column width boundary.
mod project {
    mod producer_contract {
        use yamaa_core::{
            producer_contract::{prepare_document as prepare, Cause, Error},
            schema::Document,
            value::ColumnType,
        };

        use crate::support::document::Tree;

        fn document(columns: &[(&str, &str, Option<&str>)], output: &[&str]) -> Document {
            let tree = Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
                ("domain", Tree::text("PRODUCER")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text("input.csv"))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(
                        columns
                            .iter()
                            .map(|(name, kind, label)| {
                                let mut fields = vec![
                                    ("name", Tree::text(name)),
                                    ("type", Tree::text(kind)),
                                    (
                                        "derivation",
                                        Tree::map(vec![("value", Tree::text("SRC.ID"))]),
                                    ),
                                ];
                                if let Some(label) = label {
                                    fields.push(("label", Tree::text(label)));
                                }
                                Tree::map(fields)
                            })
                            .collect(),
                    ),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("produced.csv")),
                        (
                            "columns",
                            Tree::List(output.iter().copied().map(Tree::text).collect()),
                        ),
                    ]),
                ),
            ]);
            tree.document()
        }
        const FIELDS: &[(&str, &str, Option<&str>)] = &[
            ("VALUE", "float", Some("Reported value")),
            ("ID", "int", Some("Identifier")),
            ("INTERNAL", "str", None),
        ];

        #[test]
        fn producer_width_admits_1023_and_1024_and_refuses_each_1025_boundary() {
            let names: Vec<_> = (0..1_025).map(|i| format!("C{i:04}")).collect();
            let selected: Vec<_> = names.iter().map(String::as_str).collect();
            let columns: Vec<_> = selected
                .iter()
                .map(|&name| (name, "str", Some(name)))
                .collect();
            for width in [1_023, 1_024] {
                let contract = prepare(&document(&columns[..width], &selected[..width])).unwrap();
                assert_eq!(contract.fields().len(), width);
                assert!(contract.matches_header(&selected[..width]));
            }
            assert_eq!(
                prepare(&document(&columns, &selected[..1_024])),
                Err(Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "columns",
                    limit: 1024,
                    actual: Some(1025)
                }))
            );
            assert_eq!(
                prepare(&document(&columns[..1_024], &selected)),
                Err(Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "columns",
                    limit: 1024,
                    actual: Some(1025)
                }))
            );
        }

        #[test]
        fn producer_projection_is_the_stored_order_and_internal_columns_are_excluded() {
            let contract = prepare(&document(FIELDS, &["ID", "VALUE"])).unwrap();
            assert_eq!(
                contract
                    .fields()
                    .iter()
                    .map(|f| (f.name.as_str(), f.kind, f.label.as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    ("ID", ColumnType::Int, "Identifier"),
                    ("VALUE", ColumnType::Float, "Reported value")
                ]
            );
            assert!(contract.matches_header(&["ID", "VALUE"]));
            for actual in [
                vec!["VALUE", "ID"],
                vec!["ID"],
                vec!["ID", "VALUE", "OTHER"],
                vec!["ID", "ID"],
                vec![],
            ] {
                assert!(!contract.matches_header(&actual));
            }
            assert!(contract
                .matches_typed_fields(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Float)]));
            assert!(!contract
                .matches_typed_fields(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Str)]));
            assert!(!contract
                .matches_typed_fields(&[("VALUE", ColumnType::Float), ("ID", ColumnType::Int)]));
        }

        #[test]
        fn producer_projection_rejects_empty_duplicate_and_undeclared_fields_in_order() {
            assert_eq!(
                prepare(&document(FIELDS, &[])),
                Err(Error::Invalid(vec![Cause::Empty]))
            );
            assert_eq!(
                prepare(&document(FIELDS, &["ID", "ID", "OTHER"])),
                Err(Error::Invalid(vec![
                    Cause::Duplicate {
                        position: 1,
                        field: "ID".into()
                    },
                    Cause::Declaration {
                        position: 2,
                        field: "OTHER".into(),
                        count: 0
                    }
                ]))
            );
        }

        #[test]
        fn producer_requires_one_typed_declaration_and_a_nonblank_label_per_stored_field() {
            let duplicates = [
                ("ID", "int", Some("Identifier")),
                ("ID", "str", Some("Other")),
            ];
            assert_eq!(
                prepare(&document(&duplicates, &["ID"])),
                Err(Error::Invalid(vec![Cause::Declaration {
                    position: 0,
                    field: "ID".into(),
                    count: 2
                }]))
            );
            for label in [None, Some(""), Some(" \t\n")] {
                assert_eq!(
                    prepare(&document(&[("ID", "int", label)], &["ID"])),
                    Err(Error::Invalid(vec![Cause::Label { field: "ID".into() }]))
                );
            }
        }

        /// A copy entry takes the producer's domain and each column's declared
        /// content metadata; a `str` column's length derives from its
        /// `max_length` verification where no length is declared (SPEC-9,
        /// SUB-8).
        #[test]
        fn the_contract_names_the_domain_and_each_column_s_declared_content() {
            let column = |name: &str, kind: &str, extra: Vec<(&str, Tree)>| {
                let mut fields = vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text(kind)),
                    ("label", Tree::text(name)),
                ];
                fields.extend(extra);
                Tree::map(fields)
            };
            let tree = Tree::map(vec![
                ("domain", Tree::text("ADSL")),
                (
                    "columns",
                    Tree::List(vec![
                        column(
                            "AGE",
                            "int",
                            vec![(
                                "submission",
                                Tree::map(vec![
                                    ("length", Tree::integer(8)),
                                    ("display_format", Tree::text("3.")),
                                ]),
                            )],
                        ),
                        column(
                            "SEX",
                            "str",
                            vec![
                                (
                                    "submission",
                                    Tree::map(vec![("codelist", Tree::text("SEX"))]),
                                ),
                                (
                                    "verifications",
                                    Tree::List(vec![Tree::map(vec![(
                                        "max_length",
                                        Tree::map(vec![("max", Tree::integer(1))]),
                                    )])]),
                                ),
                            ],
                        ),
                        column("TRTSDT", "date", vec![]),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![(
                        "columns",
                        Tree::List(vec![
                            Tree::text("AGE"),
                            Tree::text("SEX"),
                            Tree::text("TRTSDT"),
                        ]),
                    )]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let contract = yamaa_core::producer_contract::prepare_document(
                &Document::new(nodes, root).unwrap(),
            )
            .unwrap();
            assert_eq!(contract.domain(), "ADSL");
            let content = |index: usize| contract.fields()[index].content.clone();
            assert_eq!(content(0).length.as_deref(), Some("8"));
            assert_eq!(content(0).display_format.as_deref(), Some("3."));
            assert_eq!(content(0).codelist, None);
            assert_eq!(content(1).length.as_deref(), Some("1"));
            assert_eq!(content(1).codelist.as_deref(), Some("SEX"));
            assert_eq!(content(2), Default::default());
        }

        #[test]
        fn every_declared_logical_type_survives_without_value_inference() {
            let fields = [
                ("ID", "int", Some("ID")),
                ("TEXT", "str", Some("Text")),
                ("VALUE", "float", Some("Value")),
                ("DATE", "date", Some("Date")),
                ("TIME", "datetime", Some("Time")),
            ];
            let names = ["TIME", "DATE", "VALUE", "TEXT", "ID"];
            let contract = prepare(&document(&fields, &names)).unwrap();
            assert_eq!(
                contract.fields().iter().map(|f| f.kind).collect::<Vec<_>>(),
                vec![
                    ColumnType::DateTime,
                    ColumnType::Date,
                    ColumnType::Float,
                    ColumnType::Str,
                    ColumnType::Int
                ]
            );
        }
    }
}
