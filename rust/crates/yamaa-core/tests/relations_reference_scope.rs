//! Relations contracts: reference scope, relation ranges, relation shapes, row
//! intermediate context, source ordinal and run report. The refusals of
//! preparing and binding the row intermediate context are cases of
//! `fixtures/diagnostics/intermediates.json`; each test here proves a seam the
//! replay cannot reach, which its first comment names (#3798).
mod relations {
    mod reference_scope {
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
                    column_type: Some(Str),
                },
                Field {
                    name: "N",
                    column_type: Some(Int),
                },
            ];
            let other = [Field {
                name: "N",
                column_type: Some(Float),
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
        /// Seam: the internal `Finding` values the reference scope's `validate`
        /// returns.
        #[test]
        fn independent_scope_and_priority_truth() {
            let catalog = catalog();
            let row = scope(Phase::Row { group_by: None });
            let group = scope(Phase::Row {
                group_by: Some(&["SRC.K"]),
            });
            let column = scope(Phase::Column { groups: &[] });
            let columns = scope(Phase::Column {
                groups: &[("SRC", &["SRC.K", "SRC.N"]), ("SRC", &["SRC.K"])],
            });
            // Templates grouped over another driver leave a read of this
            // driver scalar; only this driver's own groups decide it.
            let foreign = scope(Phase::Column {
                groups: &[("OTHER", &["OTHER.N"])],
            });
            let mixed = scope(Phase::Column {
                groups: &[("SRC", &["SRC.K"]), ("OTHER", &["OTHER.N"])],
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
                ("SRC.N", Some(Int), foreign, vec![]),
                ("SRC.K", Some(Str), mixed, vec![]),
                ("SRC.N", None, mixed, vec![F::ColumnGroup]),
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
                    scope(Phase::Column {
                        groups: &[("SRC", &[])],
                    }),
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
        /// Seam: the internal `Finding` values `validate` returns for qualified
        /// queries.
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
    }
    mod relation_ranges {
        //! Inclusive range narrowing preserves original endpoint effects and exact values.
        use yamaa_core::{
            dataset::{
                CurrentField, CurrentRead, RelationRange, RelationRangeBound, RelationRangeError,
                RelationRangeFailure, RelationRangeTypes,
            },
            table::ValueRef,
            value::{FiniteFloat, ValueType},
        };
        fn bound(column: usize, kind: Option<ValueType>) -> RelationRangeBound {
            RelationRangeBound {
                column,
                identifier: if column == 0 { "LOW" } else { "HIGH" }.into(),
                kind,
            }
        }
        fn range(lower: bool, upper: bool) -> RelationRange {
            RelationRange::new(
                "intermediates[0].between".into(),
                CurrentField {
                    read: CurrentRead::Output(2),
                    identifier: "VALUE".into(),
                },
                None,
                lower.then(|| bound(0, None)),
                upper.then(|| bound(1, None)),
                yamaa_core::expression_table::Closed::Both,
            )
            .unwrap()
        }
        /// Seam: a range's admission over scripted endpoints and the refusal value it
        /// returns.
        #[test]
        fn declared_bounds_are_optional_but_cannot_both_be_omitted() {
            for (lo, hi) in [(true, false), (false, true), (true, true)] {
                let mut reads = Vec::new();
                assert!(range(lo, hi)
                    .allows(ValueRef::Int(7), |column| {
                        reads.push(column);
                        Ok::<_, ()>(ValueRef::Int(7))
                    })
                    .unwrap());
                assert_eq!(
                    reads,
                    [(lo, 0), (hi, 1)]
                        .into_iter()
                        .filter_map(|(stated, column)| stated.then_some(column))
                        .collect::<Vec<_>>()
                );
            }
            assert_eq!(
                RelationRange::new(
                    "range".into(),
                    CurrentField {
                        read: CurrentRead::Output(0),
                        identifier: "V".into()
                    },
                    None,
                    None,
                    None,
                    yamaa_core::expression_table::Closed::Both
                ),
                Err(RelationRangeError::NoBounds)
            );
        }
        /// Seam: which endpoints a range reads, logged by scripted rows.
        #[test]
        fn missing_current_or_stated_lower_stops_before_later_opaque_reads() {
            let mut reads = Vec::new();
            assert!(!range(true, true)
                .allows(ValueRef::Missing, |column| {
                    reads.push(column);
                    Err::<ValueRef<'_>, _>("unexpected read")
                })
                .unwrap());
            assert!(reads.is_empty());
            assert!(!range(true, true)
                .allows(ValueRef::Int(7), |column| {
                    reads.push(column);
                    if column == 0 {
                        Ok(ValueRef::Missing)
                    } else {
                        Err("upper must stay unread")
                    }
                })
                .unwrap());
            assert_eq!(reads, vec![0]);
            reads.clear();
            assert!(!range(true, true)
                .allows(ValueRef::Int(7), |column| {
                    reads.push(column);
                    Ok::<_, ()>(if column == 0 {
                        ValueRef::Int(1)
                    } else {
                        ValueRef::Missing
                    })
                })
                .unwrap());
            assert_eq!(reads, vec![0, 1]);
        }
        /// Seam: which endpoints a range reads, logged by scripted rows.
        #[test]
        fn both_present_endpoints_are_read_before_lower_exclusion_and_upper_comparison_is_lazy() {
            let mut reads = Vec::new();
            assert!(!range(true, true)
                .allows(ValueRef::Int(3), |column| {
                    reads.push(column);
                    Ok::<_, ()>(if column == 0 {
                        ValueRef::Int(4)
                    } else {
                        ValueRef::Str("unused incompatible comparison")
                    })
                })
                .unwrap());
            assert_eq!(reads, vec![0, 1]);
        }
        /// Seam: the original endpoint failure, by identity, and the reads before it.
        #[test]
        fn endpoint_access_failure_preserves_original_box_and_complete_read_prefix() {
            #[derive(Debug, PartialEq, Eq)]
            struct Opaque(&'static str);
            let mut original = Some(Box::new(Opaque("held source failure")));
            let address = original.as_ref().unwrap().as_ref() as *const Opaque;
            let mut reads = Vec::new();
            let error = range(true, true)
                .allows(ValueRef::Int(3), |column| {
                    reads.push(column);
                    if column == 0 {
                        Ok(ValueRef::Int(4))
                    } else {
                        Err(original.take().unwrap())
                    }
                })
                .unwrap_err();
            assert_eq!(reads, vec![0, 1]);
            let RelationRangeFailure::Access(error) = error else {
                panic!("original access owner expected")
            };
            assert_eq!(error.as_ref() as *const Opaque, address);
        }
        /// Seam: the range's runtime type refusal value and the reads before it.
        #[test]
        fn runtime_type_failure_reports_all_present_endpoint_types_after_both_reads() {
            let mut reads = Vec::new();
            let error = range(true, true)
                .allows(ValueRef::Int(7), |column| {
                    reads.push(column);
                    Ok::<_, ()>(if column == 0 {
                        ValueRef::Str("wrong")
                    } else {
                        ValueRef::Float(FiniteFloat::new(8.0).unwrap())
                    })
                })
                .unwrap_err();
            assert_eq!(reads, vec![0, 1]);
            assert_eq!(
                error,
                RelationRangeFailure::Incomparable(RelationRangeTypes {
                    value: ValueType::Int,
                    lower: Some(ValueType::Str),
                    upper: Some(ValueType::Float)
                })
            );
        }
        /// Seam: the range comparison kernel over large integers.
        #[test]
        fn large_signed_integer_bounds_compare_without_rounding_to_binary64() {
            let value = 9_007_199_254_740_993;
            assert!(!range(false, true)
                .allows(ValueRef::Int(value), |_| Ok::<_, ()>(ValueRef::Float(
                    FiniteFloat::new(9_007_199_254_740_992.0).unwrap()
                )))
                .unwrap());
            assert!(range(true, true)
                .allows(ValueRef::Int(i64::MAX), |column| Ok::<_, ()>(
                    if column == 0 {
                        ValueRef::Int(i64::MIN)
                    } else {
                        ValueRef::Int(i64::MAX)
                    }
                ))
                .unwrap());
            assert!(range(true, false)
                .allows(ValueRef::Int(i64::MIN), |_| Ok::<_, ()>(ValueRef::Float(
                    FiniteFloat::new(-9_223_372_036_854_775_808.0).unwrap()
                )))
                .unwrap());
        }
        /// Seam: the range's type refusal value before any endpoint is read.
        #[test]
        fn known_type_mismatch_is_refused_before_any_endpoint_context_exists() {
            assert_eq!(
                RelationRange::new(
                    "range".into(),
                    CurrentField {
                        read: CurrentRead::Output(0),
                        identifier: "DATE".into()
                    },
                    Some(ValueType::Date),
                    Some(bound(0, Some(ValueType::Int))),
                    Some(bound(1, Some(ValueType::Int))),
                    yamaa_core::expression_table::Closed::Both
                ),
                Err(RelationRangeError::Incomparable(RelationRangeTypes {
                    value: ValueType::Date,
                    lower: Some(ValueType::Int),
                    upper: Some(ValueType::Int)
                }))
            );
        }

        /// Seam: the projection of an internal range type refusal into its issues.
        #[test]
        fn aggregate_type_projection_omits_only_the_intermediate_owner() {
            use yamaa_core::json::Value as JsonValue;

            let types = RelationRangeTypes {
                value: ValueType::Date,
                lower: None,
                upper: Some(ValueType::Int),
            };
            let diagnostic = types.diagnostic_for("columns.V.aggregate.between", None);
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().rule
                ),
                ("validation", "incomparable_range_types", Some("LOOKUP-9"))
            );
            assert_eq!(diagnostic.spec_paths, ["columns.V.aggregate.between"]);
            assert_eq!(
                diagnostic.context,
                [
                    ("value_type".into(), JsonValue::String("date".into())),
                    ("upper_type".into(), JsonValue::String("int".into()))
                ]
                .into()
            );
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            let intermediate = types.diagnostic("intermediates[0].between", "P");
            assert_eq!(intermediate.context.len(), 3);
            assert_eq!(
                intermediate.context["intermediate"],
                JsonValue::String("P".into())
            );
        }
    }
    mod relation_shapes {
        //! Metadata never coerces an unknown derivation into a stored-column kind.
        use yamaa_core::{
            dataset::{RelationField, RelationFieldOrigin, RelationShape, RelationShapeError},
            value::{ColumnType, ValueType},
        };
        fn fields() -> Vec<RelationField> {
            vec![
                RelationField::stored(0, "ID".into(), ColumnType::Int),
                RelationField::stored(1, "LOW".into(), ColumnType::Float),
                RelationField {
                    identifier: "DERIVED".into(),
                    kind: None,
                    origin: RelationFieldOrigin::Derived(0),
                },
                RelationField {
                    identifier: "FLAG".into(),
                    kind: Some(ValueType::Bool),
                    origin: RelationFieldOrigin::Derived(1),
                },
            ]
        }
        /// Seam: the `RelationShape` a projection publishes.
        #[test]
        fn projection_restricts_reads_while_key_filter_and_order_metadata_stay_available() {
            let shape = RelationShape::new(fields(), Some(vec![2])).unwrap();
            assert_eq!(shape.visible_index("DERIVED"), Some(2));
            assert_eq!(shape.visible_index("LOW"), None);
            assert_eq!(shape.index("LOW"), Some(1));
            assert_eq!(shape.fields()[2].kind, None);
            assert_eq!(shape.fields()[3].kind, Some(ValueType::Bool));
        }
        /// Seam: the refusals `RelationShape` admission returns.
        #[test]
        fn declaration_shadowing_wrong_origins_and_invalid_projection_cannot_publish_a_shape() {
            let mut f = fields();
            f[2].identifier = "LOW".into();
            assert_eq!(
                RelationShape::new(f, None),
                Err(RelationShapeError::DuplicateIdentifier)
            );
            let mut f = fields();
            f[2].origin = RelationFieldOrigin::Derived(1);
            assert_eq!(
                RelationShape::new(f, None),
                Err(RelationShapeError::InvalidOrigin)
            );
            for projection in [vec![4], vec![1, 1], vec![usize::MAX]] {
                assert_eq!(
                    RelationShape::new(fields(), Some(projection)),
                    Err(RelationShapeError::InvalidProjection)
                );
            }
        }
        /// Seam: the refusals `RelationShape` admission returns for field kinds.
        #[test]
        fn only_derived_fields_may_have_unknown_or_boolean_logical_kind() {
            for kind in [None, Some(ValueType::Bool)] {
                let mut fields = fields();
                fields[0].kind = kind;
                assert_eq!(
                    RelationShape::new(fields, None),
                    Err(RelationShapeError::InvalidKind)
                );
            }
        }
        /// Seam: the `RelationShape` an empty projection publishes.
        #[test]
        fn empty_projection_hides_every_field_without_discarding_donor_metadata() {
            let shape = RelationShape::new(fields(), Some(vec![])).unwrap();
            assert_eq!(shape.visible_index("ID"), None);
            assert_eq!(shape.index("ID"), Some(0));
            assert_eq!(shape.fields().len(), 4);
        }
        /// Seam: the 1,024-column limit over generated fields.
        #[test]
        fn fixed_relation_width_precedes_coordinate_admission_and_keeps_exact_shape() {
            let fields = |count| {
                (0..count)
                    .map(|index| RelationField::stored(index, format!("F{index}"), ColumnType::Int))
                    .collect()
            };
            let shape = RelationShape::new(fields(1024), Some(vec![1023])).unwrap();
            assert_eq!(shape.fields().len(), 1024);
            assert_eq!(shape.visible_index("F1023"), Some(1023));
            assert_eq!(
                RelationShape::new(fields(1025), Some(vec![usize::MAX])),
                Err(RelationShapeError::Limit(
                    yamaa_core::limits::LimitExceeded {
                        resource: "columns",
                        limit: 1024,
                        actual: Some(1025)
                    }
                ))
            );
            assert_eq!(
                RelationShape::new(fields(1024), Some(vec![usize::MAX])),
                Err(RelationShapeError::InvalidProjection)
            );
        }
    }
    mod row_intermediate_context {
        //! Row admission uses original named selectors and actual completed match
        //! values. The refusals of preparing and binding this specification are
        //! cases of `fixtures/diagnostics/intermediates.json` (its `row_context`
        //! document); these tests keep the resolved plan they bind.
        use crate::support::document::Tree;
        use crate::support::resolved;
        use crate::support::specification::{header, prepare};
        use crate::support::table_schema as schema;
        use yamaa_core::{
            dataset::resolved::Grain,
            schema::{Document, DocumentNode as N},
            value::ColumnType as T,
        };

        fn doc(
            reordered: bool,
            drivers: [Option<&str>; 2],
            grouped: bool,
            field: &str,
        ) -> Document {
            let source = |name| Tree::map(vec![("value", Tree::text(name))]);
            // A template another input drives joins SRC on the key SRC
            // declares, the output key ID (D23).
            let mut inputs = vec![
                (
                    "SRC",
                    Tree::map(vec![
                        ("path", Tree::text("src.csv")),
                        ("key", Tree::List(vec![Tree::text("ID")])),
                    ]),
                ),
                ("DONOR", Tree::map(vec![("path", Tree::text("donor.csv"))])),
            ];
            if reordered {
                inputs.reverse();
            }
            let mut rows = Vec::new();
            for (id, driver) in ["first", "second"].iter().zip(drivers) {
                let mut fields = vec![
                    ("id", Tree::text(id)),
                    ("derivations", Tree::map(vec![("ROWV", source(field))])),
                ];
                if let Some(driver) = driver {
                    fields.push(("dataset", Tree::text(driver)));
                }
                if grouped {
                    fields.push((
                        "group_by",
                        Tree::List(vec![Tree::text("SRC.ID"), Tree::text("SRC.K")]),
                    ));
                }
                rows.push(Tree::map(fields));
            }
            let mut fields = header();
            fields.extend([
                ("domain", Tree::text("TEST")),
                ("input", Tree::map(inputs)),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "intermediates",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("P")),
                        ("dataset", Tree::text("DONOR")),
                        ("key", Tree::map(vec![("K", Tree::text("KEY"))])),
                        ("absent", Tree::Scalar(N::Null)),
                    ])]),
                ),
                (
                    "columns",
                    Tree::List(vec![
                        Tree::map(vec![
                            ("name", Tree::text("ID")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.ID")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("KEY")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.K")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("ROWV")),
                            ("type", Tree::text("str")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("COLV")),
                            ("type", Tree::text("str")),
                            ("derivation", source("P.V")),
                        ]),
                    ]),
                ),
                ("rows", Tree::List(rows)),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("out.csv")),
                        (
                            "columns",
                            Tree::List(vec![
                                Tree::text("ID"),
                                Tree::text("KEY"),
                                Tree::text("ROWV"),
                                Tree::text("COLV"),
                            ]),
                        ),
                    ]),
                ),
            ]);
            Tree::map(fields).document()
        }
        /// An intermediate drives rows whenever each field it exposes has one
        /// result type (ROW-2, EXPR-14): a formula over integers is an
        /// integer, padding is text.
        /// Seam: the result type the resolved plan gives each field a driving
        /// intermediate exposes.
        #[test]
        fn a_driving_intermediate_exposes_each_derived_field_as_its_one_result_type() {
            let src = schema(&[("ID", T::Int)]);
            let donor = schema(&[("ID", T::Int), ("X", T::Int)]);
            let source = |reference: &str| Tree::map(vec![("value", Tree::text(reference))]);
            for (name, value, kind) in [
                ("NEXT", Tree::text("X + 1"), T::Int),
                (
                    "PADDED",
                    Tree::map(vec![(
                        "str_pad",
                        Tree::map(vec![
                            ("source", Tree::text("X")),
                            ("width", Tree::integer(3)),
                        ]),
                    )]),
                    T::Str,
                ),
            ] {
                let mut fields = header();
                fields.extend([
                    ("domain", Tree::text("OUT")),
                    ("base", Tree::text("SRC")),
                    ("keys", Tree::List(vec![Tree::text("ID")])),
                    (
                        "input",
                        Tree::map(vec![
                            ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                            ("D", Tree::map(vec![("path", Tree::text("d.csv"))])),
                        ]),
                    ),
                    (
                        "intermediates",
                        Tree::List(vec![Tree::map(vec![
                            ("id", Tree::text("P")),
                            ("dataset", Tree::text("D")),
                            (
                                "derivations",
                                Tree::map(vec![(name, Tree::map(vec![("value", value)]))]),
                            ),
                            (
                                "columns",
                                Tree::List(vec![Tree::text("ID"), Tree::text(name)]),
                            ),
                        ])]),
                    ),
                    (
                        "output",
                        Tree::map(vec![
                            ("path", Tree::text("out.csv")),
                            ("columns", Tree::List(vec![Tree::text("ID")])),
                        ]),
                    ),
                    (
                        "columns",
                        Tree::List(vec![Tree::map(vec![
                            ("name", Tree::text("ID")),
                            ("type", Tree::text("int")),
                            ("derivation", source("SRC.ID")),
                        ])]),
                    ),
                    (
                        "rows",
                        Tree::List(vec![Tree::map(vec![
                            ("id", Tree::text("selected")),
                            ("dataset", Tree::text("P")),
                            ("derivations", Tree::map(vec![("ID", source("P.ID"))])),
                        ])]),
                    ),
                ]);
                let plan = prepare(&Tree::map(fields).document())
                    .unwrap()
                    .bind_sources(&[&src, &donor])
                    .unwrap_or_else(|error| panic!("{name}: {error:?}"));
                let item = resolved::intermediates(&plan)[0];
                assert_eq!(
                    item.derivations
                        .as_ref()
                        .unwrap()
                        .column_kind(donor.columns().len()),
                    Some(kind),
                    "{name}"
                );
            }
        }
        /// Seam: the resolved plan's source, secondary inputs, grain and reads.
        #[test]
        fn common_authored_input_needs_no_base_and_can_follow_secondary_input_in_written_order() {
            let src = schema(&[("ID", T::Str), ("K", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            for reordered in [false, true] {
                let prepared =
                    prepare(&doc(reordered, [Some("SRC"), Some("SRC")], false, "P.V")).unwrap();
                assert_eq!(prepared.source().name, "SRC");
                let schemas = if reordered {
                    vec![&donor, &src]
                } else {
                    vec![&src, &donor]
                };
                let plan = prepared.bind_sources(&schemas).unwrap();
                assert_eq!(resolved::secondary(&plan)[0].name, "DONOR");
                assert_eq!(
                    resolved::intermediates(&plan)[0]
                        .output_dependencies()
                        .collect::<Vec<_>>(),
                    [1]
                );
                for (index, template) in resolved::templates(&plan).into_iter().enumerate() {
                    assert_eq!(resolved::grain(template), &Grain::Records);
                    let row = resolved::row(&plan, index);
                    assert_eq!(
                        row.iter()
                            .map(|binding| binding.column.index())
                            .collect::<Vec<_>>(),
                        [0, 1, 2]
                    );
                    assert!(resolved::reads_intermediate(&plan, row[2].expr, 0, 1));
                }
                let columns = resolved::columns(&plan);
                assert_eq!(columns.len(), 1);
                assert!(resolved::reads_intermediate(&plan, columns[0].expr, 0, 1));
            }
        }
        /// Seam: the resolved plan's grain and the reads its templates bind.
        #[test]
        fn grouped_named_reads_bind_at_the_grain_of_their_groups() {
            let prepared = prepare(&doc(false, [Some("SRC"), Some("SRC")], true, "P.V")).unwrap();
            let src = schema(&[("ID", T::Str), ("K", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let plan = prepared.bind_sources(&[&src, &donor]).unwrap();
            for (index, template) in resolved::templates(&plan).into_iter().enumerate() {
                assert_eq!(resolved::grain(template), &Grain::Groups(vec![0, 1]));
                let row = resolved::row(&plan, index);
                assert!(resolved::reads_intermediate(&plan, row[2].expr, 0, 1));
            }
        }
        /// Seam: a column name of a million generated characters in the resolved plan.
        #[test]
        fn complete_source_metadata_is_preserved_even_without_source_rows() {
            let prepared = prepare(&doc(false, [Some("SRC"), Some("SRC")], false, "P.V")).unwrap();
            let huge = "q".repeat(1_000_000);
            let src = schema(&[("ID", T::Str), ("K", T::Str), (&huge, T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let plan = prepared.bind_sources(&[&src, &donor]).unwrap();
            assert_eq!(resolved::source(&plan).columns()[2].name, huge);
            assert_eq!(src.columns()[2].name, huge);
        }
    }
    mod source_ordinal {
        use yamaa_core::{
            source_ordinal::{Error, Ordinal},
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

        /// Seam: the admitted ordinal source: its schema, values and refusals at its
        /// limits.
        #[test]
        fn positions_and_stored_contract_are_exact_at_the_admission_boundary() {
            let original = schema();
            let admitted = Ordinal::admit(&original, 3, "REC").unwrap();
            assert_eq!(&admitted.schema().columns()[..1], original.columns());
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
            let empty = Ordinal::admit(&original, 0, "REC").unwrap();
            assert_eq!(empty.schema(), admitted.schema());
            assert_eq!(empty.value(0), None);
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
    }
    mod run_report {
        use std::collections::BTreeMap;
        use yamaa_core::json::Value as Json;
        use yamaa_core::{
            dataset::VerificationSeverity,
            run_report::{self, Error, Facts, Failure, PreparedCell},
            table::{TableAccess, ValueRef},
            value::Value,
        };

        fn held() -> run_report::Check {
            run_report::Check {
                path: "columns.AGE.verifications[0].not_missing".into(),
                name: "not_missing".into(),
                id: None,
                target: Some("AGE".into()),
                rule: "CHECK-5".into(),
                severity: VerificationSeverity::Error,
                evaluated: 8,
                failure: None,
            }
        }
        fn warning() -> run_report::Check {
            run_report::Check {
                path: "columns.AGE.verifications[1].range".into(),
                name: "range".into(),
                id: Some("é🙂".into()),
                target: Some("AGE".into()),
                rule: "CHECK-7".into(),
                severity: VerificationSeverity::Warning,
                evaluated: 8,
                failure: Some(Failure {
                    condition: "range_failed".into(),
                    count: 7,
                    keys: (0..7)
                        .map(|n| Json::Object([("ID".into(), Json::from(n))].into_iter().collect()))
                        .collect(),
                    details: BTreeMap::from([
                        ("z".into(), Json::String("é🙂\n\t\0\"\\".into())),
                        ("minimum".into(), Json::from(i64::MIN)),
                    ]),
                }),
            }
        }
        fn facts(checks: Vec<run_report::Check>) -> Facts {
            Facts {
                artifact: "out.csv".into(),
                run: run_report::Run::Succeeded { rows: 8 },
                inputs: vec![
                    run_report::Input {
                        name: "DM".into(),
                        path: "input/dm.csv".into(),
                        profile: "csv",
                        schema: None,
                        records: Some(8),
                        bytes: Some(96),
                    },
                    run_report::Input {
                        name: "LINKED".into(),
                        path: "produced.parquet".into(),
                        profile: "parquet",
                        schema: Some("producer.yaml".into()),
                        records: None,
                        bytes: None,
                    },
                ],
                rows: vec![run_report::Rows {
                    path: "base".into(),
                    driver: "DM".into(),
                    emitted: Some(8),
                    read: Some(9),
                }],
                handlers: vec![run_report::Handler {
                    path: "columns.AGE.derivation.source.absent".into(),
                    name: "absent".into(),
                    count: 0,
                }],
                columns: vec![
                    run_report::Column {
                        name: "USUBJID".into(),
                        missing: 0,
                        rows: 8,
                        max_length: Some(6),
                    },
                    run_report::Column {
                        name: "AGE".into(),
                        missing: 1,
                        rows: 8,
                        max_length: None,
                    },
                ],
                checks,
            }
        }
        /// Stand in for the adapter's canonical JSON projection: verification
        /// reads only the presence of KEYS and DETAILS text.
        fn plain(rows: Vec<Vec<PreparedCell>>) -> Vec<Vec<Value>> {
            rows.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| match cell {
                            PreparedCell::Missing => Value::Missing,
                            PreparedCell::Text(text) => Value::Str(text),
                            PreparedCell::Count(count) => Value::Int(count),
                            PreparedCell::Details(_) => Value::Str("{}".into()),
                            PreparedCell::Keys(_) => Value::Str("[]".into()),
                        })
                        .collect()
                })
                .collect()
        }
        /// Seam: the run report's column schema, which core owns.
        #[test]
        fn the_column_schema_stays_owned_by_core() {
            assert_eq!(
                run_report::COLUMNS,
                [
                    "ARTIFACT",
                    "KIND",
                    "SPEC_PATH",
                    "NAME",
                    "ID",
                    "TARGET",
                    "RULE",
                    "SEVERITY",
                    "OUTCOME",
                    "CONDITION",
                    "COUNT",
                    "TOTAL",
                    "KEYS",
                    "DETAILS"
                ]
            );
            let kinds = run_report::schema()
                .columns()
                .iter()
                .map(|column| column.kind)
                .collect::<Vec<_>>();
            assert_eq!(
                kinds
                    .iter()
                    .filter(|&&kind| kind == yamaa_core::value::ColumnType::Int)
                    .count(),
                2
            );
        }
        /// Seam: the run report core prepares from hand-built facts.
        #[test]
        fn complete_reports_prepare_kinds_in_order_with_typed_cells_and_all_keys() {
            let facts = facts(vec![held(), warning()]);
            let rows = run_report::prepare(&facts).unwrap();
            let kinds = rows.iter().map(|row| row[1].clone()).collect::<Vec<_>>();
            assert_eq!(
                kinds,
                [
                    "run", "input", "input", "rows", "handler", "column", "column", "check",
                    "check"
                ]
                .map(|kind| PreparedCell::Text(kind.into()))
            );
            assert_eq!(rows[0][2], PreparedCell::Text("output".into()));
            assert_eq!(rows[0][3], PreparedCell::Missing);
            assert_eq!(rows[0][8], PreparedCell::Text("succeeded".into()));
            assert_eq!(rows[0][10], PreparedCell::Count(8));
            assert_eq!(rows[0][11], PreparedCell::Missing);
            assert_eq!(rows[1][2], PreparedCell::Text("input.DM".into()));
            assert_eq!(
                rows[1][13],
                PreparedCell::Details(BTreeMap::from([
                    ("bytes".into(), Json::from(96)),
                    ("path".into(), Json::from("input/dm.csv")),
                    ("profile".into(), Json::from("csv")),
                ]))
            );
            assert_eq!(rows[2][10], PreparedCell::Missing);
            assert_eq!(
                rows[2][13],
                PreparedCell::Details(BTreeMap::from([
                    ("bytes".into(), Json::Null),
                    ("path".into(), Json::from("produced.parquet")),
                    ("profile".into(), Json::from("parquet")),
                    ("schema".into(), Json::from("producer.yaml")),
                ]))
            );
            assert_eq!(rows[3][2], PreparedCell::Text("base".into()));
            assert_eq!(rows[3][10], PreparedCell::Count(8));
            assert_eq!(rows[3][11], PreparedCell::Count(9));
            assert_eq!(rows[4][5], PreparedCell::Text("AGE".into()));
            assert_eq!(rows[4][6], PreparedCell::Text("LOOKUP-24".into()));
            assert_eq!(rows[4][10], PreparedCell::Count(0));
            assert_eq!(
                rows[5][13],
                PreparedCell::Details(BTreeMap::from([("max_length".into(), Json::from(6))]))
            );
            assert_eq!(rows[6][13], PreparedCell::Details(BTreeMap::new()));
            assert_eq!(rows[7][4], PreparedCell::Missing);
            assert_eq!(rows[7][8], PreparedCell::Text("held".into()));
            assert_eq!(rows[7][9], PreparedCell::Missing);
            assert_eq!(rows[7][10], PreparedCell::Count(0));
            assert_eq!(rows[7][11], PreparedCell::Count(8));
            assert_eq!(rows[7][12], PreparedCell::Missing);
            let warning = warning();
            let failure = warning.failure.as_ref().unwrap();
            assert_eq!(rows[8][10], PreparedCell::Count(7));
            assert_eq!(rows[8][12], PreparedCell::Keys(failure.keys.clone()));
            assert_eq!(rows[8][13], PreparedCell::Details(failure.details.clone()));
            let report = run_report::assemble(plain(rows));
            run_report::verify(&report, &facts).unwrap();
            assert_eq!(report.row_count(), 9);
            assert_eq!(report.cell(8, 7).unwrap(), ValueRef::Str("warning"));
        }
        /// Seam: the run report core prepares from hand-built facts.
        #[test]
        fn failed_runs_name_their_condition_and_carry_no_column_rows() {
            let mut failed = facts(vec![held()]);
            failed.run = run_report::Run::Failed {
                condition: "not_missing_failed".into(),
            };
            assert!(matches!(
                run_report::prepare(&failed),
                Err(Error::InvalidRecord)
            ));
            failed.columns.clear();
            failed.rows[0].emitted = None;
            failed.rows[0].read = None;
            let rows = run_report::prepare(&failed).unwrap();
            assert_eq!(rows[0][8], PreparedCell::Text("failed".into()));
            assert_eq!(rows[0][9], PreparedCell::Text("not_missing_failed".into()));
            assert_eq!(rows[0][10], PreparedCell::Missing);
            assert_eq!(rows[3][10], PreparedCell::Missing);
            assert_eq!(rows[3][11], PreparedCell::Missing);
            run_report::verify(&run_report::assemble(plain(rows)), &failed).unwrap();
        }
        /// Seam: the refusal of hand-built facts before any report row.
        #[test]
        fn malformed_facts_refuse_before_rows_are_returned() {
            let mut malformed = Vec::new();
            for change in 0..5 {
                let mut check = warning();
                let failure = check.failure.as_mut().unwrap();
                match change {
                    0 => failure.count = 9,
                    1 => {
                        failure.details.insert("keys".into(), Json::Null);
                    }
                    2 => failure.keys.push(Json::from(1)),
                    // Number owns JSON grammar; a numeric context still requires
                    // a finite binary64 value for fractional/exponent spellings.
                    3 => {
                        failure
                            .details
                            .insert("bad".into(), Json::Number("1e999".parse().unwrap()));
                    }
                    _ => failure.count = 0,
                }
                malformed.push(facts(vec![check]));
            }
            malformed.push(facts(vec![held(), held()]));
            let mut collision = facts(vec![held()]);
            collision.handlers[0].path = held().path;
            malformed.push(collision);
            let mut uneven = facts(vec![]);
            uneven.rows[0].read = None;
            malformed.push(uneven);
            for facts in malformed {
                assert!(matches!(
                    run_report::prepare(&facts),
                    Err(Error::InvalidRecord)
                ));
            }
        }
        /// Seam: the run report's verification of hand-built rows.
        #[test]
        fn verification_rejects_any_row_that_breaks_the_contract() {
            let facts = facts(vec![held(), warning()]);
            let rows = plain(run_report::prepare(&facts).unwrap());
            let tamper = |row: usize, column: usize, value: Value| {
                let mut rows = rows.clone();
                rows[row][column] = value;
                run_report::verify(&run_report::assemble(rows), &facts)
            };
            assert!(tamper(0, 0, Value::Str("other.csv".into())).is_err());
            assert!(tamper(1, 1, Value::Str("rows".into())).is_err());
            assert!(tamper(8, 2, Value::Str(held().path)).is_err());
            assert!(tamper(2, 2, Value::Missing).is_err());
            assert!(tamper(0, 8, Value::Str("held".into())).is_err());
            assert!(tamper(7, 8, Value::Str("failed".into())).is_err());
            assert!(tamper(7, 7, Value::Str("fatal".into())).is_err());
            assert!(tamper(3, 10, Value::Int(10)).is_err());
            assert!(tamper(7, 10, Value::Int(1)).is_err());
            assert!(tamper(8, 12, Value::Missing).is_err());
            assert!(tamper(7, 12, Value::Str("[]".into())).is_err());
            assert!(tamper(5, 10, Value::Str("0".into())).is_err());
            assert!(tamper(5, 13, Value::Missing).is_err());
            let mut short = rows.clone();
            short.pop();
            assert!(run_report::verify(&run_report::assemble(short), &facts).is_err());
            assert!(run_report::verify(&run_report::assemble(rows), &facts).is_ok());
        }
        /// A handler row names the output column its path belongs to,
        /// however its file writes the row template (P-CLR-2), and cites the
        /// rule of the condition its handler answers: an `absent` row
        /// LOOKUP-24 (D204).
        /// Seam: the run report core prepares from hand-built facts.
        #[test]
        fn handler_rows_cite_the_condition_their_handler_answers() {
            for (path, handler, target, rule) in [
                (
                    "columns.A.derivation.mapping.missing",
                    "missing",
                    Some("A"),
                    None,
                ),
                (
                    "rows.week-2.derivations.V.mapping.unmapped",
                    "unmapped",
                    Some("V"),
                    Some("COLUMN-10"),
                ),
                (
                    "rows['week 2'].derivations.V.mapping.unmapped",
                    "unmapped",
                    Some("V"),
                    Some("COLUMN-10"),
                ),
                // A quoted row id may hold what would end its step unquoted.
                (
                    "rows['a].derivations.B'].derivations.V.mapping.unmapped",
                    "unmapped",
                    Some("V"),
                    Some("COLUMN-10"),
                ),
                (
                    "rows['week 2.derivations.V.mapping.unmapped",
                    "unmapped",
                    None,
                    Some("COLUMN-10"),
                ),
                (
                    "columns.A.derivation.str_extract.no_match",
                    "no_match",
                    Some("A"),
                    Some("COLUMN-10"),
                ),
                (
                    "columns.A.derivation.case[0].then.source.multiple_matches",
                    "multiple_matches",
                    Some("A"),
                    Some("LOOKUP-10"),
                ),
                (
                    "columns.A.derivation.cut.missing",
                    "missing",
                    Some("A"),
                    None,
                ),
                (
                    "columns.S.derivation.str_template.missing",
                    "missing",
                    Some("S"),
                    Some("COLUMN-10"),
                ),
                (
                    "columns.A.derivation.source.absent",
                    "absent",
                    Some("A"),
                    Some("LOOKUP-24"),
                ),
                (
                    "rows.event.derivations.A.first_available.sources[1].absent",
                    "absent",
                    Some("A"),
                    Some("LOOKUP-24"),
                ),
                (
                    "columns.D.derivation.impute.missing",
                    "missing",
                    Some("D"),
                    None,
                ),
                (
                    "columns.D.derivation.impute.invalid",
                    "invalid",
                    Some("D"),
                    Some("TIME-13"),
                ),
                (
                    "columns.P.derivation.precision.missing",
                    "missing",
                    Some("P"),
                    None,
                ),
                (
                    "columns.T.derivation.to_date.invalid",
                    "invalid",
                    Some("T"),
                    Some("TIME-13"),
                ),
                (
                    "columns.Q.derivation.precision.invalid",
                    "invalid",
                    Some("Q"),
                    Some("TIME-13"),
                ),
                (
                    "columns.E.derivation.duration_value.invalid",
                    "invalid",
                    Some("E"),
                    Some("TIME-17"),
                ),
                (
                    "columns.S.derivation.dtc.invalid",
                    "invalid",
                    Some("S"),
                    Some("TIME-19"),
                ),
                (
                    "columns.N.derivation.unconvertible",
                    "unconvertible",
                    Some("N"),
                    Some("VALUE-3"),
                ),
                (
                    "intermediates.CODED.derivations.PICK.unconvertible",
                    "unconvertible",
                    None,
                    Some("VALUE-3"),
                ),
                (
                    "columns.N.derivation.round.invalid",
                    "invalid",
                    Some("N"),
                    None,
                ),
            ] {
                assert_eq!(run_report::handler_target(path), target, "{path}");
                assert_eq!(
                    run_report::handler_condition(path, handler)
                        .and_then(|code| code.definition().rule),
                    rule,
                    "{path}"
                );
            }
        }
        /// Seam: the run report core prepares from hand-built facts.
        #[test]
        fn canonical_json_numbers_and_temporal_values_are_retained_as_typed_details() {
            let mut check = warning();
            let details = BTreeMap::from([
                (
                    "date".into(),
                    Json::from(yamaa_core::value::Value::Date(
                        "2025-01-02".parse().unwrap(),
                    )),
                ),
                (
                    "datetime".into(),
                    Json::from(yamaa_core::value::Value::DateTime(
                        "2025-01-02T03:04".parse().unwrap(),
                    )),
                ),
                (
                    "integer_float".into(),
                    Json::from(yamaa_core::value::Value::float(1.0)),
                ),
                (
                    "negzero".into(),
                    Json::from(yamaa_core::value::Value::float(-0.0)),
                ),
                (
                    "wide".into(),
                    Json::Number(
                        "12345678901234567890123456789012345678901234567890"
                            .parse()
                            .unwrap(),
                    ),
                ),
            ]);
            check.failure.as_mut().unwrap().details = details.clone();
            let rows = run_report::prepare(&facts(vec![check])).unwrap();
            // The governed byte spelling of these exact typed values is
            // asserted by the adapter projection tests.
            assert_eq!(rows[7][13], PreparedCell::Details(details));
        }
        /// Seam: the run report core prepares from hand-built facts.
        #[test]
        fn complete_check_sets_keep_all_rows_and_metadata_without_copy_allowances() {
            let checks: Vec<_> = (0..6_000)
                .map(|index| {
                    let mut check = held();
                    check.path = format!("columns.AGE.verifications[{index}].not_missing");
                    check.id = Some(format!("check {index}"));
                    check
                })
                .collect();
            let facts = facts(checks.clone());
            let rows = run_report::prepare(&facts).unwrap();
            assert_eq!(rows.len(), checks.len() + 7);
            for (index, check) in checks.iter().enumerate() {
                assert_eq!(rows[index + 7][2], PreparedCell::Text(check.path.clone()));
                assert_eq!(
                    rows[index + 7][4],
                    PreparedCell::Text(check.id.clone().unwrap())
                );
                assert_eq!(rows[index + 7][11], PreparedCell::Count(8));
            }
            run_report::verify(&run_report::assemble(plain(rows)), &facts).unwrap();
        }
        /// Seam: the run report's depth boundary over generated details.
        #[test]
        fn recursive_json_details_use_the_shared_semantic_depth_boundary() {
            use yamaa_core::limits::Limits;
            for depth in [
                Limits::SEMANTIC_DEPTH - 1,
                Limits::SEMANTIC_DEPTH,
                Limits::SEMANTIC_DEPTH + 1,
            ] {
                let mut detail = Json::Null;
                for _ in 0..depth {
                    detail = Json::Array(vec![detail]);
                }
                let mut check = held();
                check.failure = Some(Failure {
                    condition: "not_missing_failed".into(),
                    count: 1,
                    keys: vec![],
                    details: BTreeMap::from([("nested".into(), detail)]),
                });
                let result = run_report::prepare(&facts(vec![check]));
                if depth <= Limits::SEMANTIC_DEPTH {
                    assert!(result.is_ok());
                } else {
                    let Err(Error::Limit(error)) = result else {
                        panic!("deep details must refuse before encoding")
                    };
                    assert_eq!(error.resource, "semantic_depth");
                    assert_eq!(error.limit, Limits::SEMANTIC_DEPTH);
                    assert_eq!(error.actual, Some(depth));
                }
            }
        }
        /// The table a saved report decodes to, an input row's DETAILS
        /// standing in as `bytes=<n>`, which [`details`] reads back.
        fn saved(facts: &Facts) -> run_report::Report {
            let rows = run_report::prepare(facts).unwrap();
            run_report::assemble(
                rows.into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|cell| match cell {
                                PreparedCell::Details(details) if details.contains_key("bytes") => {
                                    Value::Str(match details["bytes"].as_u64() {
                                        Some(bytes) => format!("bytes={bytes}"),
                                        None => "bytes=null".into(),
                                    })
                                }
                                cell => plain(vec![vec![cell]]).remove(0).remove(0),
                            })
                            .collect()
                    })
                    .collect(),
            )
        }
        fn details(text: &str) -> Option<Option<usize>> {
            match text.strip_prefix("bytes=")? {
                "null" => Some(None),
                bytes => bytes.parse().ok().map(Some),
            }
        }
        /// A saved report reads back as the run, its row count or failed
        /// condition, and each input's records and bytes (PROJECT-15); a
        /// table that is not a run report reads as nothing.
        /// Seam: the `Recorded` value read back from a report saved from hand-built facts.
        #[test]
        fn a_saved_report_reads_back_its_run_and_its_inputs() {
            let facts = facts(vec![held(), warning()]);
            let recorded = run_report::recorded(&saved(&facts), details).unwrap();
            assert_eq!(
                recorded,
                run_report::Recorded {
                    artifact: "out.csv".into(),
                    run: run_report::Run::Succeeded { rows: 8 },
                    inputs: vec![
                        run_report::RecordedInput {
                            name: "DM".into(),
                            records: Some(8),
                            bytes: Some(96),
                        },
                        run_report::RecordedInput {
                            name: "LINKED".into(),
                            records: None,
                            bytes: None,
                        },
                    ],
                }
            );
            let mut failed = facts.clone();
            failed.run = run_report::Run::Failed {
                condition: "duplicate_key".into(),
            };
            failed.columns.clear();
            let recorded = run_report::recorded(&saved(&failed), details).unwrap();
            assert_eq!(recorded.run, failed.run);
            // Not a run report: DETAILS without a byte length, a first row
            // that is no run row, another artifact, or other columns.
            assert_eq!(run_report::recorded(&saved(&facts), |_| None), None);
            let rows = run_report::prepare(&facts).unwrap();
            let mut moved = plain(rows.clone());
            moved.swap(0, 1);
            assert_eq!(
                run_report::recorded(&run_report::assemble(moved), details),
                None
            );
            let mut renamed = plain(rows);
            renamed[1][0] = Value::Str("other.csv".into());
            assert_eq!(
                run_report::recorded(&run_report::assemble(renamed), details),
                None
            );
            let other = yamaa_core::typed_csv::PreparedTypes::new(&[])
                .unwrap()
                .convert(crate::support::text_table(
                    &["ARTIFACT", "KIND"],
                    &[&[Some("out.csv"), Some("run")]],
                ))
                .unwrap();
            assert_eq!(run_report::recorded(&other, details), None);
        }
    }
}
