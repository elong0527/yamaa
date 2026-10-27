//! Independently authored native diagnostic contexts. Each test here proves
//! a seam core's issue-only readers cannot reach, which its first comment
//! names: a projection's refusal, a fact borrowed by address, a count of
//! `usize::MAX`, a context of megabytes, a rendered message or a composed
//! model. The findings these families report are replay cases: a derive
//! binding's in `aggregates.json`, an expression's text in `grammar.json`, a
//! lock's facts in `runtime.json` and a cited terminology standard's in
//! `submission.json`.
mod diagnostics {
    mod diagnostic {
        /// A text context value.
        fn text(value: &str) -> yamaa_core::json::Value {
            yamaa_core::json::Value::String(value.into())
        }

        mod aggregate {
            use super::text;
            use yamaa_core::{
                dataset::AggregatePhase,
                diagnostic::{AggregateFinding as Finding, ConditionCode, ProjectionError},
                json::Value as V,
                value::{Value, ValueType},
            };
            /// Seam: the projection's `InvalidContext` refusals of constructed
            /// findings, and a 1,000,000-byte group value kept at its address.
            #[test]
            fn invalid_causes_refuse_and_large_group_values_stay_complete() {
                for types in [
                    &[ValueType::Int, ValueType::Float][..],
                    &[ValueType::Str, ValueType::Str][..],
                    &[][..],
                ] {
                    assert_eq!(
                        Finding::IncomparableSources {
                            path: "aggregate",
                            expression: "MAX(V)",
                            source: "V",
                            types
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                }
                for count in [0, 1] {
                    assert_eq!(
                        Finding::MultipleRecords {
                            path: "aggregate",
                            expression: "ONLY(V)",
                            phase: AggregatePhase::Derivation,
                            count,
                            row: None,
                            group: &[]
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                }
                let huge = "x".repeat(1_000_000);
                let group = vec![("D.ID".into(), Value::Str(huge.clone()))];
                let address = huge.as_ptr();
                let diagnostic = Finding::MultipleRecords {
                    path: "aggregate",
                    expression: "ONLY(D.V)",
                    phase: AggregatePhase::RowConstruction,
                    count: 2,
                    row: Some("total"),
                    group: &group,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.spec_paths, ["aggregate"]);
                assert_eq!(diagnostic.code, ConditionCode::AggregateRowMultipleRecords);
                assert_eq!(
                    diagnostic.context,
                    [
                        ("expr".into(), text("ONLY(D.V)")),
                        ("reducer".into(), text("ONLY")),
                        ("record_count".into(), V::from(2)),
                        ("row".into(), text("total")),
                        (
                            "group".into(),
                            V::Object([("D.ID".into(), text(&huge))].into_iter().collect())
                        ),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
                assert_eq!(huge.as_ptr(), address);
                assert_eq!(group[0].1, Value::Str(huge));
            }
            /// Seam: a record count of `usize::MAX`, on 64-bit targets only.
            #[test]
            #[cfg(target_pointer_width = "64")]
            fn record_counts_keep_the_complete_unsigned_range() {
                let diagnostic = Finding::MultipleRecords {
                    path: "aggregate",
                    expression: "ONLY(V)",
                    phase: AggregatePhase::Derivation,
                    count: usize::MAX,
                    row: None,
                    group: &[],
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["record_count"], V::from(usize::MAX));
            }
        }
        mod binding {
            use yamaa_core::column_dependencies;
            /// Seam: the projection of `analyze`'s internal diagnostics over
            /// malformed graph metadata.
            #[test]
            fn malformed_dependency_metadata_has_no_projection_or_panic() {
                let analysis =
                    column_dependencies::analyze(&[Some(vec![1]), Some(vec![0])], &[], false)
                        .unwrap();
                for finding in analysis.diagnostics {
                    assert!(finding.specification_diagnostic(&[], &[]).is_none());
                }
            }
        }
        mod dataset {
            use super::text;
            use yamaa_core::json::Value as V;
            use yamaa_core::{
                diagnostic::{ConditionCode, DatasetFinding as F, ProjectionError},
                expression::Reducer,
                temporal::{Date, DateTime, Precision},
                value::{Value, ValueType},
            };
            /// Seam: constructed findings: a datetime over a date collected to
            /// the year, which no case's value writes, and the spelling of each
            /// non-numeric type beside it.
            #[test]
            fn temporal_spelling_and_nonnumeric_reduction_types_remain_complete() {
                let date = Date::new(1, 1, 1, Precision::Year).unwrap();
                let datetime =
                    Value::DateTime(DateTime::new(date, 1, 2, 3, Precision::Day).unwrap());
                let diagnostic = F::BaselineAmbiguity {
                    path: "baseline",
                    column: "B",
                    date: &datetime,
                    count: 2,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["date"], text("0001-01-01T01:02:03"));
                for (actual, spelling) in [
                    (ValueType::Str, "str"),
                    (ValueType::Bool, "bool"),
                    (ValueType::Date, "date"),
                    (ValueType::DateTime, "datetime"),
                ] {
                    let diagnostic = F::ReductionInputType {
                        path: "aggregate",
                        expression: "SUM(A)",
                        reducer: Reducer::Sum,
                        source: "A",
                        actual,
                    }
                    .diagnostic()
                    .unwrap();
                    assert_eq!(diagnostic.context["actual"], text(spelling));
                }
            }
            /// Seam: a 3,000,000-byte context value kept at its address, and
            /// the projection's `InvalidContext` refusals.
            #[test]
            fn large_context_and_invalid_causes_preserve_borrowed_facts() {
                let huge = "x".repeat(3_000_000);
                let address = huge.as_ptr();
                let cause = F::MultipleMatches {
                    path: "source",
                    dataset: "DM",
                    intermediate: &huge,
                    count: 2,
                };
                let diagnostic = cause.diagnostic().unwrap();
                assert_eq!(diagnostic.code, ConditionCode::DatasetMultipleMatches);
                assert_eq!(diagnostic.spec_paths, ["source"]);
                assert_eq!(
                    diagnostic.context,
                    [
                        ("dataset".into(), text("DM")),
                        ("intermediate".into(), text(&huge)),
                        ("match_count".into(), V::from(2)),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
                assert_eq!(huge.as_ptr(), address);
                assert_eq!(huge.len(), 3_000_000);
                for count in [0, 1] {
                    assert_eq!(
                        F::MultipleValues {
                            path: "p",
                            identifier: "A",
                            count
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                    assert_eq!(
                        F::MultipleMatches {
                            path: "p",
                            dataset: "DM",
                            intermediate: "I",
                            count
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                }
                let wrong = Value::Str("2024-01-01".into());
                assert_eq!(
                    F::BaselineAmbiguity {
                        path: "p",
                        column: "A",
                        date: &wrong,
                        count: 2
                    }
                    .diagnostic(),
                    Err(ProjectionError::InvalidContext)
                );
                assert_eq!(
                    F::ReductionInputType {
                        path: "p",
                        expression: "SUM(A)",
                        reducer: Reducer::Sum,
                        source: "A",
                        actual: ValueType::Int
                    }
                    .diagnostic(),
                    Err(ProjectionError::InvalidContext)
                );
            }
            /// Seam: counts of `usize::MAX`, on 64-bit targets only.
            #[test]
            #[cfg(target_pointer_width = "64")]
            fn counts_above_i64_remain_mathematical_diagnostic_integers() {
                let diagnostic = F::MultipleValues {
                    path: "p",
                    identifier: "A",
                    count: usize::MAX,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["value_count"], V::from(usize::MAX));
                let diagnostic = F::MultipleMatches {
                    path: "p",
                    dataset: "DM",
                    intermediate: "I",
                    count: usize::MAX,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["match_count"], V::from(usize::MAX));
            }
        }
        mod grammar {
            use super::text;
            use yamaa_core::expression::{compile, CompileError, Context as Field};
            /// Seam: the identifiers of the compiled predicate. The refusals of
            /// `AGE >> 65` and `A >`, with their text and position, are the
            /// cases
            /// `grammar.json::a_doubled_greater_than_sign_is_an_invalid_predicate_at_the_second_sign`
            /// and
            /// `grammar.json::an_incomplete_predicate_is_an_invalid_expression`.
            #[test]
            fn arithmetic_in_a_comparison_operand_is_admitted_with_its_names() {
                for (text, names) in [
                    ("SBPSTD - SBPSUP <= -20", vec!["SBPSTD", "SBPSUP"]),
                    ("VS.SBPSTD - VS.SBPSUP < 0", vec!["VS.SBPSTD", "VS.SBPSUP"]),
                    ("ASTDT <= TRTEDT + 30", vec!["ASTDT", "TRTEDT"]),
                ] {
                    let compiled = compile(
                        text,
                        Field::Predicate,
                        "columns.OHYPO.derivation.case[0].when",
                    )
                    .unwrap();
                    assert_eq!(compiled.identifiers().collect::<Vec<_>>(), names, "{text}");
                }
            }
            /// Seam: a failure projected over another expression's text, which
            /// `expression_refusal` never passes, so the function's name can
            /// come only from the failure.
            #[test]
            fn a_function_finding_carries_its_own_written_name() {
                for (field, written, function) in [
                    (Field::Derivation, "  foo(1)", "foo"),
                    (Field::Aggregate, "  sumx(SRC.ID)", "sumx"),
                    (Field::Derivation, "  str_contains(A, 'x')", "str_contains"),
                ] {
                    let Err(CompileError::Invalid { failure, .. }) =
                        compile(written, field, "columns.VALUE")
                    else {
                        panic!("prohibited function");
                    };
                    let d =
                        failure.diagnostic(field, "columns.VALUE", "\u{03b2}\u{03b2}\u{03b2}", 0);
                    assert_eq!(d.definition().condition, "prohibited_function");
                    assert_eq!(d.context["function"], text(function));
                    assert_eq!(d.context["expr"], text("\u{03b2}\u{03b2}\u{03b2}"));
                }
            }
        }
        mod inheritance {
            use yamaa_core::{
                diagnostic::{ConditionCode, InheritanceFinding as F},
                inheritance::Named,
                json::Value as JsonValue,
            };
            /// Seam: the projection's `InvalidContext` refusals of
            /// contradictory cycle carriers, beside the record of a consistent
            /// one.
            #[test]
            fn contradictory_cycle_carriers_refuse_without_relabeling_the_cause() {
                for path in [
                    vec![],
                    vec!["entry".into()],
                    vec!["entry".into(), "other".into()],
                ] {
                    assert_eq!(
                        F::Cycle {
                            path: &path,
                            returns_to_entry: true,
                            named: Named::Parent,
                        }
                        .diagnostic(),
                        Err(yamaa_core::diagnostic::ProjectionError::InvalidContext)
                    );
                }
                let path = vec!["entry".into(), "parent".into(), "entry".into()];
                let diagnostic = F::Cycle {
                    path: &path,
                    returns_to_entry: true,
                    named: Named::Parent,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.code, ConditionCode::InheritanceCycle);
                assert_eq!(diagnostic.spec_paths, ["parent"]);
                assert_eq!(
                    diagnostic.context,
                    [
                        (
                            "reason".into(),
                            JsonValue::String("parent_chain_returns_to_entry".into())
                        ),
                        ("source".into(), JsonValue::String("parent".into())),
                        (
                            "cycle".into(),
                            JsonValue::Array(
                                ["entry", "parent", "entry"]
                                    .map(|name| JsonValue::String(name.into()))
                                    .into()
                            )
                        ),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
            }
        }
        mod odm {
            use super::text;
            use crate::support::diagnostic::rendered;
            use yamaa_core::{
                dataset::AggregatePhase,
                diagnostic::{ConditionCode, OdmFinding as F},
                json::Value as V,
                value::Value,
            };
            /// Seam: the rendered message of 400-character escaped facts,
            /// beside the records of constructed findings.
            #[test]
            fn complete_odm_projection_preserves_nested_values_and_escaped_metadata() {
                let large = "\"".repeat(400);
                let names = [large.clone()];
                let diagnostic = F::AmbiguousField {
                    dataset: "D",
                    field: "ItemOID",
                    stored: &names,
                }
                .diagnostic();
                let definition = diagnostic.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.rule,
                        diagnostic.spec_paths.as_slice(),
                        &diagnostic.context,
                        diagnostic.source_span,
                        &diagnostic.operand_route,
                    ),
                    (
                        "validation",
                        "odm_schema_field_ambiguous",
                        Some("NAME-8"),
                        ["input.D.path".to_string()].as_slice(),
                        &[
                            ("dataset".into(), text("D")),
                            ("field".into(), text("ItemOID")),
                            ("stored".into(), V::Array(vec![text(&large)])),
                        ]
                        .into(),
                        None,
                        &None,
                    )
                );
                rendered(&diagnostic);
                let value = "\\".repeat(400);
                let differ = [("FormOID".into(), vec![Value::Str(value.clone())])];
                let diagnostic = F::NotUnique {
                    path: "odm",
                    item: "D.Item",
                    row: None,
                    phase: AggregatePhase::Derivation,
                    scope: &[],
                    differ: &differ,
                    records: 2,
                }
                .diagnostic();
                assert_eq!(diagnostic.code, ConditionCode::OdmNotUnique);
                assert_eq!(diagnostic.spec_paths, ["odm"]);
                assert_eq!(
                    diagnostic.context,
                    [
                        ("item".into(), text("D.Item")),
                        ("row".into(), V::Null),
                        ("scope".into(), V::Object(Default::default())),
                        ("records".into(), V::from(2)),
                        (
                            "differ".into(),
                            V::Object(
                                [("FormOID".into(), V::Array(vec![text(&value)]))]
                                    .into_iter()
                                    .collect()
                            )
                        ),
                        ("repeated".into(), V::Bool(false)),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
            }
            /// Seam: each refusal's rendered message, which an issue row does
            /// not hold.
            ///
            /// Each refusal of the ODM source `read_odm` reads renders its
            /// condition's message from its facts (ODM-1, P-MSG-1).
            #[test]
            fn each_odm_source_refusal_renders_its_message() {
                use yamaa_core::{
                    diagnostic::OdmReadFinding,
                    odm_items::{Definition, Document, Error, Layout, Limit, Member, ValueRefusal},
                };
                for (refusal, message) in [
                    (
                        Error::Archive {
                            reason: "truncated TAR header",
                        },
                        "the archive edc/odm.xml cannot be read: truncated TAR header",
                    ),
                    (
                        Error::Compression { format: "xz" },
                        "edc/odm.xml uses xz compression, but only plain and gzip TAR archives are read",
                    ),
                    (
                        Error::Member(Member::Candidates { count: 2 }),
                        "no member of edc/odm.xml can be read: TAR input must contain exactly one XML \
                         member or receive archive_member",
                    ),
                    (
                        Error::Member(Member::Matches {
                            name: "study/odm.xml".into(),
                            count: 0,
                        }),
                        "no member of edc/odm.xml can be read: TAR input contains 0 entries named \
                         \"study/odm.xml\"",
                    ),
                    (
                        Error::Limit(Limit::Depth(64)),
                        "edc/odm.xml exceeds a read limit: XML depth exceeds 64",
                    ),
                    (
                        Error::Malformed {
                            position: 41,
                            reason: "unclosed element".into(),
                        },
                        "edc/odm.xml is not well-formed XML at byte 41: unclosed element",
                    ),
                    (
                        Error::Doctype,
                        "edc/odm.xml uses XML the reader does not support: DOCTYPE declarations are not \
                         supported",
                    ),
                    (
                        Error::Encoding {
                            encoding: "UTF-16".into(),
                        },
                        "edc/odm.xml uses XML the reader does not support: ODM XML must be encoded as \
                         UTF-8 or US-ASCII, not UTF-16",
                    ),
                    (
                        Error::Document(Document::Namespace { namespace: None }),
                        "edc/odm.xml is not a supported ODM 1.3 or 2.0 document: unsupported ODM \
                         namespace (none)",
                    ),
                    (
                        Error::Layout(Layout::Nested("FormData")),
                        "edc/odm.xml holds clinical data the item table cannot represent: nested FormData \
                         is not supported",
                    ),
                    (
                        Error::MissingAttribute {
                            element: "ItemData",
                            attribute: "ItemOID",
                        },
                        "ItemData in edc/odm.xml requires a non-empty ItemOID",
                    ),
                    (
                        Error::AmbiguousAttribute {
                            name: "StudySubjectID",
                        },
                        "edc/odm.xml has more than one namespaced attribute named StudySubjectID",
                    ),
                    (
                        Error::InvalidAttribute {
                            element: "ItemData".into(),
                            attribute: "IsNull",
                            value: "maybe".into(),
                        },
                        "ItemData in edc/odm.xml has the unsupported IsNull value 'maybe'",
                    ),
                    (
                        Error::InvalidAttribute {
                            element: "ODM".into(),
                            attribute: "FileType",
                            value: "Archive".into(),
                        },
                        "ODM in edc/odm.xml has the unsupported FileType value 'Archive'",
                    ),
                    (
                        Error::MetadataConflict {
                            kind: Definition::Item,
                            study: "S".into(),
                            version: "M".into(),
                            oid: "I.AGE".into(),
                        },
                        "edc/odm.xml gives the item I.AGE of S/M different names",
                    ),
                    (
                        Error::Value {
                            item: "I.AGE".into(),
                            refusal: ValueRefusal::MultipleChildren,
                        },
                        "the ItemData I.AGE in edc/odm.xml holds a value the item table cannot represent: \
                         has multiple Value children",
                    ),
                    (
                        Error::Transactional {
                            element: "ODM".into(),
                            attribute: "FileType",
                            value: "Transactional".into(),
                        },
                        "edc/odm.xml is not a snapshot: ODM declares FileType Transactional",
                    ),
                    (
                        Error::Transactional {
                            element: "ItemGroupData".into(),
                            attribute: "TransactionType",
                            value: "Remove".into(),
                        },
                        "edc/odm.xml is not a snapshot: ItemGroupData declares TransactionType Remove",
                    ),
                    (
                        Error::NoClinicalData,
                        "edc/odm.xml holds no ClinicalData",
                    ),
                    (
                        Error::Option {
                            option: "max_xml_depth",
                            minimum: 8,
                            value: 7,
                        },
                        "max_xml_depth has the value 7, which is not permitted",
                    ),
                ] {
                    let finding = OdmReadFinding {
                        refusal: &refusal,
                        path: "edc/odm.xml",
                        dataset: None,
                    };
                    assert_eq!(finding.diagnostic().message(), message, "{refusal:?}");
                }
            }
        }
        mod parquet {
            use yamaa_core::{
                parquet_source::{self as p, Field},
                value::ColumnType,
            };
            fn field(name: &str) -> Field {
                Field {
                    name: name.into(),
                    kind: Some(ColumnType::Int),
                    stored_type: "int64".into(),
                }
            }
            /// Seam: how many fields the codec inspects before it refuses.
            #[test]
            fn alignment_and_early_profile_failure_do_not_inspect_later_codec_metadata() {
                let mut inspected = 0;
                let fields = ["I", ""].into_iter().map(|name| {
                    inspected += 1;
                    field(name)
                });
                assert_eq!(p::columns(fields, 1).unwrap_err(), p::Error::Invalid);
                assert_eq!(inspected, 0);
                let mut inspected = 0;
                let fields = ["", "I"].into_iter().map(|name| {
                    inspected += 1;
                    field(name)
                });
                assert_eq!(
                    p::columns(fields, 2).unwrap_err(),
                    p::Error::EmptyName { field: 1 }
                );
                assert_eq!(inspected, 1);
            }
        }
        mod predicate_evaluation {
            use super::text;
            use std::convert::Infallible;
            use yamaa_core::json::Value as V;
            use yamaa_core::{
                diagnostic::EvaluationDiagnosticError as P,
                expression::{
                    compile, CompiledExpression, Condition, Context, ErrorKind, EvaluationError,
                    Resolver, Step as Route,
                },
                value::{Selection, Value},
            };
            fn plan(expression: &str) -> CompiledExpression {
                compile(expression, Context::Predicate, "rows[2].filter").unwrap()
            }
            /// A host error the resolver hands back unchanged.
            struct Payload {
                original: Box<u8>,
            }
            /// Reads `v` and `low`, and fails reading `high` with its payload.
            struct Port {
                payload: Option<Payload>,
                trace: Vec<String>,
            }
            impl Resolver for Port {
                type Error = Payload;
                fn resolve(&mut self, name: &str) -> Result<Selection, Payload> {
                    self.trace.push(name.into());
                    match name {
                        "v" => Ok(Selection::Present(Value::Bool(true))),
                        "low" => Ok(Selection::Present(Value::Int(0))),
                        "high" => Err(self.payload.take().unwrap()),
                        _ => Ok(Selection::Absent),
                    }
                }
            }
            /// Seam: the parser's own syntax facts and message, kept at their
            /// address, and a 1,400,000-byte expression at position
            /// `usize::MAX`.
            #[test]
            fn original_context_projection_admits_all_fields_and_retains_rich_parser_facts() {
                for (expression, rule, character, detail) in [
                    ("STR_CONTAINS(ID, '(')", "FORMULA-8", 17, "pattern_byte"),
                    ("ID = DATE '2026-02-30'", "FORMULA-1", 10, "temporal_error"),
                ] {
                    let error = compile(
                        expression,
                        Context::Predicate,
                        "columns.VALUE.derivation.row_number.window.filter",
                    )
                    .unwrap_err();
                    let yamaa_core::expression::CompileError::Invalid {
                        failure,
                        position,
                        message,
                    } = &error
                    else {
                        panic!("independent rejected original form")
                    };
                    assert_eq!(position.character, character);
                    let detail_before = failure.syntax_diagnostic();
                    assert!(detail_before.context.contains_key(detail));
                    let message_address = message.as_ptr();
                    let finding = failure.diagnostic(
                        Context::Predicate,
                        "columns.VALUE.derivation.row_number.window.filter",
                        expression,
                        position.character,
                    );
                    assert_eq!(finding.definition().rule, Some(rule));
                    assert_eq!(
                        finding.context,
                        [
                            ("expr".into(), text(expression)),
                            ("predicate".into(), text(expression)),
                            ("position".into(), V::from(character)),
                        ]
                        .into()
                    );
                    assert_eq!(finding.source_span, None);
                    assert_eq!(finding.operand_route, None);
                    assert_eq!(failure.syntax_diagnostic(), detail_before);
                    assert_eq!(message.as_ptr(), message_address);
                }
                // ESCAPE owns FORMULA-7 under the one syntax condition.
                let yamaa_core::expression::CompileError::Invalid {
                    failure: failure @ yamaa_core::expression::Failure::InvalidEscape,
                    position,
                    ..
                } = compile(
                    "ID LIKE '%' ESCAPE 'xx'",
                    Context::Predicate,
                    "rows[0].filter",
                )
                .unwrap_err()
                else {
                    panic!("invalid ESCAPE width")
                };
                let finding = failure.diagnostic(
                    Context::Predicate,
                    "rows[0].filter",
                    "ID LIKE '%' ESCAPE 'xx'",
                    position.character,
                );
                assert_eq!(finding.definition().rule, Some("FORMULA-7"));
                assert_eq!(
                    finding.context["position"],
                    V::Number(("19").parse().expect("canonical integer"))
                );
                let huge = "\0".repeat(1_400_000);
                let projected = yamaa_core::expression::Failure::Syntax.diagnostic(
                    Context::Predicate,
                    "rows[0].filter",
                    &huge,
                    usize::MAX,
                );
                assert_eq!(projected.spec_paths, ["rows[0].filter"]);
                assert_eq!(
                    projected.context,
                    [
                        ("expr".into(), text(&huge)),
                        ("predicate".into(), text(&huge)),
                        ("position".into(), V::from(usize::MAX)),
                    ]
                    .into()
                );
            }
            /// Seam: a resolver's opaque payload kept at its address, and the
            /// names the resolver is asked for.
            #[test]
            fn upper_resolution_outranks_lower_type_error_and_projection_never_retries() {
                let original = Box::new(41);
                let pointer = &*original as *const u8;
                let mut port = Port {
                    payload: Some(Payload { original }),
                    trace: vec![],
                };
                let p = plan("v BETWEEN low AND high");
                let Err(error) = p.evaluate_truth(&mut port) else {
                    panic!("opaque upper error")
                };
                assert_eq!(port.trace, ["v", "low", "v", "high"]);
                assert_eq!(error.route, [Route::Upper]);
                for _ in 0..2 {
                    let Err(P::Resolution(payload)) = error.diagnostic() else {
                        panic!("borrowed original")
                    };
                    assert_eq!(&*payload.original as *const u8, pointer);
                }
                assert_eq!(port.trace, ["v", "low", "v", "high"]);
                let ErrorKind::Resolution {
                    identifier,
                    error: payload,
                } = error.kind
                else {
                    panic!("still original")
                };
                assert_eq!(identifier, "high");
                assert_eq!(&*payload.original as *const u8, pointer);
            }
            /// Seam: a constructed evaluation error's routes, 65,536 steps
            /// long, and a 1,400,000-byte identifier.
            #[test]
            fn complete_long_context_and_full_width_routes_are_preserved() {
                let mut error: EvaluationError<Infallible> = EvaluationError {
                    spec_path: "rows[0].filter".into(),
                    expression: "unchanged".into(),
                    route: vec![
                        Route::Child,
                        Route::Left,
                        Route::Right,
                        Route::Value,
                        Route::Item(usize::MAX),
                        Route::Lower,
                        Route::RepeatedValue,
                        Route::Upper,
                        Route::Pattern,
                        Route::Unary,
                        Route::LeftOperand,
                        Route::RightOperand,
                    ],
                    source_span: None,
                    kind: ErrorKind::Condition(Condition::DanglingEscape),
                };
                let d = error.diagnostic().unwrap();
                assert_eq!(
                    d.operand_route,
                    Some(vec![
                        "Child".into(),
                        "Left".into(),
                        "Right".into(),
                        "Value".into(),
                        format!("Item({})", usize::MAX),
                        "Lower".into(),
                        "RepeatedValue".into(),
                        "Upper".into(),
                        "Pattern".into(),
                        "unary".into(),
                        "left".into(),
                        "right".into()
                    ])
                );
                error.route = vec![Route::Left; 65_536];
                let projected = error.diagnostic().unwrap();
                assert_eq!(projected.operand_route, Some(vec!["Left".into(); 65_536]));
                // The dangling escape names the predicate it is written in.
                assert_eq!(
                    projected.context,
                    [
                        ("expr".into(), text("unchanged")),
                        ("reason".into(), text("LIKE pattern has a dangling escape")),
                    ]
                    .into()
                );
                error.route = vec![];
                let identifier = "\0".repeat(1_400_000);
                error.kind = ErrorKind::Condition(Condition::UnknownField {
                    identifier: identifier.clone(),
                });
                let projected = error.diagnostic().unwrap();
                assert_eq!(
                    projected.context,
                    [("identifier".into(), text(&identifier))].into()
                );
                assert_eq!(projected.operand_route, Some(vec![]));
                assert_eq!(error.expression, "unchanged");
            }
        }
        mod project_activation {
            use crate::support::project_call_compiler as support;
            use std::rc::Rc;
            use yamaa_core::{
                dataset::RowIdentity,
                diagnostic::ConditionCode,
                function_invocation::{FailureKind, InvocationFailure},
                function_signature::ProjectFunctionIdentity,
                project_activation::{BindingFailure, TestFailure},
                project_diagnostics::{
                    self as projection, DefinitionSource, Diagnostic, Error, Fact, HostDetails,
                },
                value::{FiniteFloat, Value},
            };
            /// Seam: projected facts borrowed by address from the failure.
            ///
            /// A projection borrows the facts it names from the failure it
            /// projects: the invoked function's name, and the result a
            /// conformance case produced.
            #[test]
            fn projected_facts_borrow_the_failure_they_name() {
                let failure = InvocationFailure {
                    identity: ProjectFunctionIdentity {
                        name: "id".into(),
                        call: "pkg.id".into(),
                    },
                    kind: FailureKind::<()>::BooleanResult,
                };
                let diagnostic =
                    projection::invocation("columns.X.derivation", &failure, None, &[], None)
                        .unwrap();
                let Fact::Text(name) = diagnostic.context["function"] else {
                    panic!("borrowed function")
                };
                assert!(std::ptr::eq(name.as_ptr(), failure.identity.name.as_ptr()));
                let function = support::function("id");
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions[0]",
                }];
                let failures: [TestFailure<()>; 1] = [TestFailure::Result {
                    function: 0,
                    case: 0,
                    actual: Value::Int(8),
                    expected: function.definition().tests[0].result.clone(),
                }];
                let diagnostic = projection::test_diagnostics(&definitions, &failures, &[None])
                    .unwrap()
                    .remove(0);
                let Fact::Scalar(held) = diagnostic.context["actual"] else {
                    panic!("borrowed actual")
                };
                assert!(
                    matches!(&failures[0], TestFailure::Result { actual, .. } if std::ptr::eq(held, actual))
                );
            }
            /// Seam: an opaque host payload kept at its address, with its
            /// reference count.
            #[test]
            fn opaque_payloads_remain_borrowed_with_original_precedence() {
                struct Payload(Rc<()>); // no Clone, Display, Send or Sync requirement
                let payload = Payload(Rc::new(()));
                let identity = ProjectFunctionIdentity {
                    name: String::new(),
                    call: String::new(),
                };
                let failure = InvocationFailure {
                    identity: identity.clone(),
                    kind: FailureKind::CallFailed(&payload),
                };
                let Err(Error::Opaque(held)) =
                    projection::invocation("", &failure, None, &[], None)
                else {
                    panic!("opaque failure precedence")
                };
                assert!(std::ptr::eq(*held, &payload));
                assert_eq!(Rc::strong_count(&held.0), 1);
                let function = support::function("id");
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions[0]",
                }];
                let failure = [BindingFailure {
                    function: 0,
                    error: &payload,
                }];
                let Err(Error::Opaque(held)) =
                    projection::binding_diagnostics(&definitions, &failure, &[None])
                else {
                    panic!("opaque binding")
                };
                assert!(std::ptr::eq(*held, &payload));
            }
            /// Seam: host details of 2,048 repetitions borrowed by address in
            /// three projections.
            #[test]
            fn complete_host_details_are_borrowed_in_runtime_binding_and_nested_case_diagnostics() {
                let function = support::function("id");
                let definition = function.definition();
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions[0]",
                }];
                let class = "Exception".repeat(2048);
                let message = "exact\0\u{e9}\u{1f642}".repeat(2048);
                let reason = "invalid_native_result".repeat(1024);
                let returned = "returned\0\u{e9}".repeat(2048);
                for (host, fields) in [
                    (
                        HostDetails::Exception {
                            class: &class,
                            message: &message,
                        },
                        [
                            ("host_error", class.as_str()),
                            ("host_message", message.as_str()),
                        ],
                    ),
                    (
                        HostDetails::Rejected {
                            reason: &reason,
                            returned: Some(&returned),
                        },
                        [("reason", reason.as_str()), ("returned", returned.as_str())],
                    ),
                ] {
                    let failure = InvocationFailure {
                        identity: ProjectFunctionIdentity {
                            name: "id".into(),
                            call: "project.id".into(),
                        },
                        kind: FailureKind::CallFailed(()),
                    };
                    let failures = [TestFailure::Invocation {
                        function: 0,
                        case: 0,
                        error: failure,
                    }];
                    let TestFailure::Invocation { error: failure, .. } = &failures[0] else {
                        unreachable!()
                    };
                    let context = [
                        ("function", Fact::Text("id")),
                        ("call", Fact::Text("project.id")),
                        (fields[0].0, Fact::Text(fields[0].1)),
                        (fields[1].0, Fact::Text(fields[1].1)),
                    ]
                    .into();
                    let diagnostic = projection::invocation(
                        "columns.X.derivation",
                        failure,
                        None,
                        &[],
                        Some(host),
                    )
                    .unwrap();
                    assert_eq!(
                        diagnostic,
                        Diagnostic {
                            code: ConditionCode::RuntimeFunctionCallFailed,
                            spec_path: "columns.X.derivation".into(),
                            context,
                        }
                    );
                    for (key, text) in fields {
                        let Fact::Text(held) = diagnostic.context[key] else {
                            panic!("borrowed host detail")
                        };
                        assert!(std::ptr::eq(held.as_ptr(), text.as_ptr()));
                    }
                    let binding = [BindingFailure {
                        function: 0,
                        error: (),
                    }];
                    assert_eq!(
                        projection::binding_diagnostics(&definitions, &binding, &[Some(host)])
                            .unwrap(),
                        [Diagnostic {
                            code: ConditionCode::FunctionFileInvalid,
                            spec_path: "functions[0].implementations.python".into(),
                            // The same host facts, and the callable named
                            // invalid (FUNC-2) unless the host gave a reason.
                            context: {
                                let mut context = diagnostic.context.clone();
                                context
                                    .entry("reason")
                                    .or_insert(Fact::Text("invalid_callable"));
                                context
                            },
                        }]
                    );
                    assert_eq!(
                        projection::test_diagnostics(&definitions, &failures, &[Some(host)])
                            .unwrap(),
                        [Diagnostic {
                            code: ConditionCode::FunctionConformanceFailed,
                            spec_path: "functions[0].tests[0]".into(),
                            context: [
                                ("function", Fact::Text("id")),
                                ("call", Fact::Text("project.id")),
                                ("case", Fact::Text("normal")),
                                (
                                    "args",
                                    Fact::Object(
                                        [("x", Fact::Scalar(&definition.tests[0].args[0].1))]
                                            .into()
                                    )
                                ),
                                ("expected", Fact::Scalar(&definition.tests[0].result)),
                                ("comparison_decimals", Fact::Integer(4)),
                                (
                                    "reason",
                                    Fact::Text("the case failed before it produced a result")
                                ),
                                (
                                    "failure",
                                    Fact::Object(
                                        [
                                            ("phase", Fact::Text("derivation")),
                                            ("condition", Fact::Text("function_call_failed")),
                                            ("rule", Fact::Text("FUNC-8")),
                                            ("context", Fact::Object(diagnostic.context)),
                                        ]
                                        .into()
                                    )
                                ),
                            ]
                            .into(),
                        }]
                    );
                }
            }
            /// Seam: the projection's `InvalidContext` refusals.
            #[test]
            fn late_contradictions_return_no_partial_report() {
                let function = support::function("id");
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions[0]",
                }];
                let actual = Value::Int(8);
                let expected = &function.definition().tests[0].result;
                let failures: [TestFailure<()>; 2] = [
                    TestFailure::Result {
                        function: 0,
                        case: 0,
                        actual: actual.clone(),
                        expected: expected.clone(),
                    },
                    TestFailure::Result {
                        function: 0,
                        case: 99,
                        actual: actual.clone(),
                        expected: expected.clone(),
                    },
                ];
                assert!(matches!(
                    projection::test_diagnostics(&definitions, &failures, &[None, None],),
                    Err(Error::InvalidContext)
                ));
                let wrong = Value::Int(9);
                let failures: [TestFailure<()>; 1] = [TestFailure::Result {
                    function: 0,
                    case: 0,
                    actual: actual.clone(),
                    expected: wrong.clone(),
                }];
                assert!(matches!(
                    projection::test_diagnostics(&definitions, &failures, &[None],),
                    Err(Error::InvalidContext)
                ));
            }
            /// Seam: a nested failure's facts and a row's keys borrowed by
            /// address.
            #[test]
            fn nested_invocation_and_row_keys_preserve_borrowed_scalar_geometry() {
                let function = support::function("id");
                let identity = ProjectFunctionIdentity {
                    name: "id".into(),
                    call: function.call().into(),
                };
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions[0]",
                }];
                let failures: [TestFailure<()>; 1] = [TestFailure::Invocation {
                    function: 0,
                    case: 0,
                    error: InvocationFailure {
                        identity: identity.clone(),
                        kind: FailureKind::MissingRequired {
                            parameter: "x".into(),
                        },
                    },
                }];
                let diagnostics =
                    projection::test_diagnostics(&definitions, &failures, &[None]).unwrap();
                let Fact::Object(held) = &diagnostics[0].context["failure"] else {
                    panic!("nested borrowed diagnostic")
                };
                assert_eq!(held["rule"], Fact::Text("FUNC-5"));
                let Fact::Object(context) = &held["context"] else {
                    panic!("nested invocation")
                };
                assert_eq!(context["argument"], Fact::Text("x"));
                let row = [
                    Value::Int(i64::MIN),
                    Value::Int(i64::MAX),
                    Value::Float(FiniteFloat::new(-0.0).unwrap()),
                    Value::Str("\u{e9}\0\u{1f642}".into()),
                ];
                let row = RowIdentity {
                    position: 0,
                    values: row.to_vec(),
                };
                let failure = InvocationFailure::<(), ProjectFunctionIdentity> {
                    identity,
                    kind: FailureKind::BooleanResult,
                };
                let diagnostic = projection::invocation(
                    "columns.X.derivation",
                    &failure,
                    Some(&row),
                    &["MIN", "MAX", "ZERO", "TEXT"],
                    None,
                )
                .unwrap();
                let Fact::Sequence(keys) = &diagnostic.context["keys"] else {
                    panic!("row keys")
                };
                let [Fact::Object(keys)] = keys.as_slice() else {
                    panic!("one exact key observation")
                };
                for (name, value) in ["MIN", "MAX", "ZERO", "TEXT"].into_iter().zip(&row.values) {
                    let Fact::Scalar(held) = keys[name] else {
                        panic!("scalar key")
                    };
                    assert!(std::ptr::eq(held, value));
                }
            }
        }
        mod project_lock {
            use super::text;
            use yamaa_core::{
                diagnostic::{
                    lock_diagnostics, ConditionCode, LockDiagnosticError as E, LockFinding,
                    LockOrigins, LockReason as R,
                },
                json::Value as V,
            };
            fn origins() -> LockOrigins<'static> {
                LockOrigins {
                    source: "study/environment.yaml",
                    entry: "study/project.yaml",
                    field: "lock",
                    lock_source: "held/uv.lock",
                    written: "../uv.lock",
                }
            }
            fn finding(reason: R, expected: &[&str], actual: Option<&str>) -> LockFinding {
                LockFinding {
                    package: "program".into(),
                    reason,
                    expected: expected.iter().map(|v| (*v).into()).collect(),
                    actual: actual.map(str::to_owned),
                }
            }
            /// Seam: `lock_diagnostics`'s `InvalidContext` refusals, which no
            /// reader returns.
            #[test]
            fn contradictory_shapes_refuse_the_complete_report() {
                assert_eq!(lock_diagnostics(None, &[]).unwrap(), vec![]);
                let first = || finding(R::VersionMismatch, &["1.0"], Some("2.0"));
                assert_eq!(lock_diagnostics(None, &[first()]), Err(E::InvalidContext));
                // A lock declared at no field has no place to be reported.
                let fieldless = LockOrigins {
                    field: "",
                    ..origins()
                };
                assert_eq!(
                    lock_diagnostics(Some(fieldless), &[first()]),
                    Err(E::InvalidContext)
                );
                for bad in [
                    finding(R::DistributionNotIdentified, &["1"], None),
                    finding(R::VersionNotLocked, &[], Some("1")),
                    finding(R::AmbiguousLockVersion, &["1"], None),
                    finding(R::InvalidLockedVersion, &["1", "2"], None),
                    finding(R::PackageNotInstalled, &["1"], Some("2")),
                    finding(R::InvalidInstalledVersion, &[], Some("2")),
                    finding(R::VersionMismatch, &["1"], None),
                    LockFinding {
                        package: "bad\0package".into(),
                        ..first()
                    },
                    LockFinding {
                        package: String::new(),
                        ..first()
                    },
                ] {
                    assert_eq!(
                        lock_diagnostics(Some(origins()), &[first(), bad]),
                        Err(E::InvalidContext)
                    );
                }
            }
            /// Seam: generated facts of 65,537 bytes, 2,050 findings and 65,537
            /// versions.
            #[test]
            fn long_lock_facts_and_all_ordered_versions_remain_complete() {
                let source = "\0".repeat(65_537);
                let package = "p".repeat(2049);
                let actual = "é".repeat(1025);
                let records = [LockFinding {
                    package: package.clone(),
                    reason: R::VersionMismatch,
                    expected: vec!["1".into()],
                    actual: Some(actual.clone()),
                }];
                let report = lock_diagnostics(
                    Some(LockOrigins {
                        source: &source,
                        ..origins()
                    }),
                    &records,
                )
                .unwrap();
                assert_eq!(
                    report,
                    [yamaa_core::diagnostic::Diagnostic {
                        code: ConditionCode::ProjectLockMismatch,
                        spec_paths: vec!["lock".into()],
                        context: [
                            ("source".into(), text(&source)),
                            ("entry".into(), text("study/project.yaml")),
                            ("lock_source".into(), text("held/uv.lock")),
                            ("lock".into(), text("../uv.lock")),
                            ("package".into(), text(&package)),
                            ("reason".into(), text("version_mismatch")),
                            ("expected".into(), V::Array(vec![text("1")])),
                            ("actual".into(), text(&actual)),
                        ]
                        .into(),
                        source_span: None,
                        operand_route: None,
                    }]
                );
                let many = (0..2050)
                    .map(|_| finding(R::VersionNotLocked, &[], None))
                    .collect::<Vec<_>>();
                let projected = lock_diagnostics(Some(origins()), &many).unwrap();
                assert_eq!(projected.len(), 2050);
                assert!(projected
                    .iter()
                    .all(|diagnostic| diagnostic.context["reason"] == text("version_not_locked")));
                let versions = [LockFinding {
                    package: "program".into(),
                    reason: R::AmbiguousLockVersion,
                    expected: vec!["1".into(); 65_537],
                    actual: None,
                }];
                let projected = lock_diagnostics(Some(origins()), &versions).unwrap();
                assert_eq!(
                    projected[0].context["expected"],
                    V::Array(vec![text("1"); 65_537])
                );
                assert_eq!(projected[0].context["actual"], V::Null);
            }
        }
        mod submission {
            use crate::support::{
                document::Tree,
                roots as rt,
                submission::{catalogue, column, one, project, specification},
            };
            use std::convert::Infallible;
            use yamaa_core::{
                diagnostic::ConditionCode,
                json::Value as JsonValue,
                project_terminology as ct, submission_transport,
                table::{CellError, Column, TableAccess, TableSchema, ValueRef},
                value::{ColumnType, Value},
            };
            /// Seam: the number of facts the refusal names, which no case
            /// states without deriving all eight.
            ///
            /// CT-11: transport admission refuses a published value outside
            /// the study's dataset inventory with eight facts; the case
            /// `submission.json::a_published_value_outside_the_dataset_inventory_fails`
            /// compares its path and four of them.
            #[test]
            fn an_inventory_refusal_at_transport_names_eight_facts() {
                struct Held(TableSchema, Value);
                impl TableAccess for Held {
                    type Error = Infallible;
                    fn schema(&self) -> &TableSchema {
                        &self.0
                    }
                    fn row_count(&self) -> usize {
                        1
                    }
                    fn cell(
                        &self,
                        row: usize,
                        column: usize,
                    ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                        match (row, column) {
                            (0, 0) => Ok((&self.1).into()),
                            _ => Err(CellError::OutOfBounds { row, column }),
                        }
                    }
                }
                let mut spec = specification();
                spec.set(
                    &["columns", "0", "submission", "inventory_vocabulary"],
                    Tree::boolean(true),
                );
                let study = one(project(), spec, &ct::Catalogue::admit(vec![]).unwrap()).unwrap();
                let table = Held(
                    TableSchema::new(vec![Column {
                        name: "VALUE".into(),
                        kind: ColumnType::Str,
                    }])
                    .unwrap(),
                    Value::Str("UNKNOWN_DATASET".into()),
                );
                let finding =
                    submission_transport::admit(&study, &study.datasets()[0], &table).unwrap_err();
                assert_eq!(
                    (finding.code, finding.context.len()),
                    (ConditionCode::SubmissionInventoryFailed, 8),
                    "{:?}",
                    finding.context
                );
            }
            /// Seam: an 8,388,609-byte codelist value kept in the composed
            /// study, and the redundancy finding over it.
            #[test]
            fn large_binding_values_are_retained_and_a_restating_verification_is_redundant() {
                // Held semantic metadata has no cumulative copy allowance. File loaders
                // enforce their own per-file boundary before producing these arenas.
                let long = "X".repeat(8_388_609);
                let lists = catalogue(false, Value::Str(long.clone()));
                let mut spec = specification();
                spec.set(
                    &["columns", "0", "submission", "codelist"],
                    Tree::text("LIST"),
                );
                let study = one(project(), spec.clone(), &lists).unwrap();
                assert_eq!(study.codelists().len(), 1);
                assert_eq!(
                    study.codelists()[0].declaration.items.as_ref().unwrap()[0].value,
                    Value::Str(long.clone())
                );
                // A list of items states the column's value set; an equal
                // `allowed_values` beside it states it twice.
                spec.set(
                    &["columns", "0", "verifications"],
                    Tree::list(vec![Tree::map(vec![(
                        "allowed_values",
                        Tree::map(vec![("values", Tree::list(vec![Tree::text(&long)]))]),
                    )])]),
                );
                let findings = one(project(), spec, &lists).unwrap_err();
                assert_eq!(findings.len(), 1);
                assert_eq!(
                    findings[0].code,
                    ConditionCode::CodelistAllowedValuesRedundant
                );
                assert_eq!(findings[0].definition().rule, Some("CT-9"));
                assert_eq!(
                    findings[0].path,
                    "sdtm.specs[0].columns.VALUE.verifications[0].allowed_values"
                );
                assert_eq!(
                    findings[0].context["stated_by"],
                    JsonValue::String("sdtm.specs[0].columns.VALUE.submission.codelist".into())
                );
            }
            /// Seam: the composed study's standards and codelists, a model no
            /// reader returns.
            ///
            /// DEFINE-9 and DEFINE-10: a terminology standard's identifier is
            /// `CT_` and its publishing set. Sources that cite one standard
            /// share its definition, and a source the document binds nothing
            /// from cites none (CT-10). Another release or another standard
            /// name under that publishing set is refused at its own source
            /// instead of being numbered: the cases
            /// `submission.json::a_later_release_of_a_cited_terminology_standard_is_refused_at_its_publishing_set`
            /// and
            /// `submission.json::another_standard_name_under_a_cited_publishing_set_is_refused_at_its_publishing_set`.
            #[test]
            fn sources_citing_one_terminology_standard_share_one_definition() {
                const NCI: &str = "CDISC/NCI";
                let source = |name: &str, version: &str, list: &str| {
                    rt::Source {
                        standard: Some(rt::Standard {
                            name: name.into(),
                            publishing_set: "SDTM".into(),
                            version: version.into(),
                        }),
                        codelists: vec![rt::Codelist {
                            id: list.into(),
                            name: format!("Independent list {list}"),
                            data_type: rt::CodelistType::Text,
                            extensible: false,
                            alias: None,
                            format_name: None,
                            external: None,
                            items: Some(vec![rt::Item {
                                value: Value::Str("X".into()),
                                decode: None,
                                rank: None,
                                alias: None,
                                extended: false,
                            }]),
                        }],
                    }
                    .root()
                };
                let bound = |name: &str, list: &str| {
                    let mut column = column(name);
                    column.set(&["submission", "codelist"], Tree::text(list));
                    column
                };
                let mut spec = specification();
                spec.set(
                    &["columns"],
                    Tree::list(vec![bound("VALUE", "FIRST"), bound("OTHER", "SECOND")]),
                );
                spec.set(
                    &["output", "columns"],
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("OTHER")]),
                );
                let compose = |sources| {
                    one(
                        project(),
                        spec.clone(),
                        &ct::Catalogue::admit(sources).unwrap(),
                    )
                };
                let (cited, later) = ("2026-03-27", "2026-06-26");
                let study = compose(vec![
                    source(NCI, cited, "FIRST"),
                    source(NCI, later, "UNBOUND"),
                    source(NCI, cited, "SECOND"),
                ])
                .unwrap();
                // The model the Define-XML encoder writes (the adapters'
                // `tests/submission_encoders.rs` checks the document): one
                // terminology standard that both bound codelists cite.
                let terminology: Vec<_> = study
                    .standards()
                    .iter()
                    .filter(|standard| standard.id == "CT_SDTM")
                    .collect();
                assert_eq!(terminology.len(), 1, "{:?}", study.standards());
                assert_eq!(terminology[0].version, cited);
                assert_eq!(
                    study
                        .codelists()
                        .iter()
                        .map(|list| (list.declaration.id.name.as_str(), list.standard.as_deref()))
                        .collect::<Vec<_>>(),
                    [("FIRST", Some("CT_SDTM")), ("SECOND", Some("CT_SDTM"))]
                );
                assert!(study
                    .standards()
                    .iter()
                    .all(|standard| standard.version != later));
            }
            /// Seam: a table port of 10,000,001 rows that panics if a cell is
            /// read.
            #[test]
            fn transport_refuses_oversized_geometry_before_reading_cells() {
                use yamaa_core::limits::Limits;
                struct Oversized(TableSchema);
                impl TableAccess for Oversized {
                    type Error = Infallible;
                    fn schema(&self) -> &TableSchema {
                        &self.0
                    }
                    fn row_count(&self) -> usize {
                        Limits::TABLE_ROWS + 1
                    }
                    fn cell(
                        &self,
                        _: usize,
                        _: usize,
                    ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                        panic!("geometry must be admitted before table access")
                    }
                }
                let lists = ct::Catalogue::admit(vec![]).unwrap();
                let study = one(project(), specification(), &lists).unwrap();
                let table = Oversized(
                    TableSchema::new(vec![Column {
                        name: "VALUE".into(),
                        kind: ColumnType::Str,
                    }])
                    .unwrap(),
                );
                let finding =
                    submission_transport::admit(&study, &study.datasets()[0], &table).unwrap_err();
                assert_eq!(finding.code, ConditionCode::ApplicationRejected);
                assert_eq!(finding.path, "datasets.D0.transport");
                // An engine fault names its step, closed code and bound.
                assert_eq!(
                    finding.context,
                    [
                        ("step".into(), JsonValue::String("define".into())),
                        ("code".into(), JsonValue::String("resource_limit".into())),
                        (
                            "detail".into(),
                            JsonValue::String(
                                "table_rows: required Some(10000001), limit 10000000".into(),
                            ),
                        ),
                        ("resource".into(), JsonValue::String("table_rows".into())),
                        ("limit".into(), JsonValue::from(10_000_000)),
                        ("actual".into(), JsonValue::from(10_000_001)),
                    ]
                    .into()
                );
            }
        }
        mod verification {
            use crate::support::{
                project_call_compiler::{specification_with_verifications, Tree as T},
                specification::prepare,
            };
            use yamaa_core::{
                dataset::{CheckRecord, RowIdentity},
                diagnostic::ConditionCode,
                project_diagnostics::{Context, Fact},
                schema::DocumentNode,
                specification::PreparedSpecification,
                value::Value,
                verification_diagnostics::{project, Error, Phase},
            };
            struct Row(Vec<Value>);
            /// A specification declaring a `unique` check of warning
            /// severity, an `all_or_none`, an `assert` and a `row_count`.
            fn compiled() -> PreparedSpecification {
                let list = || T::List(vec![T::text("VAL"), T::text("ID")]);
                let spec = specification_with_verifications(
                    vec![("VAL", "int", T::text("SRC.VAL"))],
                    vec![],
                    vec![
                        T::map(vec![(
                            "unique",
                            T::map(vec![
                                ("columns", list()),
                                ("id", T::text("u")),
                                ("severity", T::text("warning")),
                            ]),
                        )]),
                        T::map(vec![(
                            "all_or_none",
                            T::map(vec![("columns", list()), ("id", T::text("a"))]),
                        )]),
                        T::map(vec![(
                            "assert",
                            T::map(vec![("require", T::text("VAL > 0")), ("id", T::text("b"))]),
                        )]),
                        T::map(vec![(
                            "row_count",
                            T::map(vec![
                                ("min", T::Scalar(DocumentNode::Integer("1".into()))),
                                ("id", T::text("r")),
                            ]),
                        )]),
                    ],
                    Some("warnings.csv"),
                );
                prepare(&spec).unwrap()
            }
            fn record(code: ConditionCode, path: &str, rows: &[Row]) -> CheckRecord {
                CheckRecord {
                    path: path.into(),
                    code,
                    evaluated_count: 9,
                    failed_count: 1,
                    output_rows: 9,
                    offending_rows: rows
                        .iter()
                        .map(|row| RowIdentity {
                            position: 0,
                            values: row.0.clone(),
                        })
                        .collect(),
                    codelist: None,
                    inventory: None,
                    row_count: None,
                    entries: None,
                    denominator: None,
                }
            }
            fn object<'a>(fact: &'a Fact<'a>) -> &'a Context<'a> {
                match fact {
                    Fact::Object(fields) => fields,
                    _ => panic!("complete context"),
                }
            }
            /// Seam: the observation a warning projects, which no issue row
            /// holds, with counts of `usize::MAX` and keys borrowed by address.
            #[test]
            fn warning_display_is_bounded_but_all_keys_and_exact_scalars_remain_in_order() {
                let compiled = compiled();
                let rows: Vec<_> = (0..7)
                    .map(|n| {
                        Row(vec![if n == 0 {
                            Value::Int(i64::MAX)
                        } else {
                            Value::Int(n)
                        }])
                    })
                    .collect();
                let mut record = record(
                    ConditionCode::VerificationUniqueFailed,
                    "verifications[0].unique",
                    &rows,
                );
                record.evaluated_count = usize::MAX;
                record.output_rows = usize::MAX;
                // Unique failure count is repeated groups, rather than all group members.
                record.failed_count = 2;
                let records = [record];
                let projected =
                    project(&compiled, &records, Phase::Verification, "study.yaml").unwrap();
                assert!(projected.diagnostics.is_empty());
                let observation = object(&projected.observations[0]);
                assert_eq!(observation["specification"], Fact::Text("study.yaml"));
                assert_eq!(observation["severity"], Fact::Text("warning"));
                assert_eq!(observation["verification_id"], Fact::Text("u"));
                assert_eq!(observation["evaluated_count"], Fact::Unsigned(usize::MAX));
                let failure = object(&observation["failure"]);
                assert_eq!(failure["condition"], Fact::Text("unique_failed"));
                assert_eq!(failure["rule"], Fact::Text("CHECK-10"));
                let context = object(&failure["context"]);
                assert_eq!(
                    context["columns"],
                    Fact::Sequence(vec![Fact::Text("VAL"), Fact::Text("ID")])
                );
                assert_eq!(context["failure_count"], Fact::Unsigned(2));
                let Fact::Sequence(display) = &context["keys"] else {
                    panic!("display keys")
                };
                assert_eq!(display.len(), 5);
                let Fact::Sequence(retained) = &failure["offending_keys"] else {
                    panic!("complete keys")
                };
                assert_eq!(retained.len(), 7);
                let Fact::Scalar(value) = object(&retained[0])["ID"] else {
                    panic!("borrowed key")
                };
                assert!(std::ptr::eq(value, &records[0].offending_rows[0].values[0]));
                let Fact::Sequence(logged) = &object(&failure["log_context"])["keys"] else {
                    panic!("log keys")
                };
                assert_eq!(logged.len(), 7);
            }
            /// Seam: projected facts and observations of constructed check
            /// records, with a count of `usize::MAX`.
            #[test]
            fn output_missing_keys_are_not_rejected_or_lost_and_row_count_keeps_full_width() {
                let compiled = compiled();
                let rows = [Row(vec![Value::Missing])];
                let records = [record(
                    ConditionCode::OutputMissingKeyFailed,
                    "keys[0]",
                    &rows,
                )];
                let result = project(&compiled, &records, Phase::Output, "s.yaml").unwrap();
                // The output phase observes no verification.
                assert!(result.observations.is_empty());
                let context = &result.diagnostics[0].context;
                assert_eq!(context["column"], Fact::Text("ID"));
                let Fact::Sequence(keys) = &context["keys"] else {
                    panic!("missing keys")
                };
                assert_eq!(object(&keys[0])["ID"], Fact::Scalar(&Value::Missing));
                let mut count = record(
                    ConditionCode::VerificationRowCountFailed,
                    "verifications[3].row_count",
                    &[],
                );
                count.output_rows = usize::MAX;
                count.evaluated_count = 1;
                let counts = [count];
                let result = project(&compiled, &counts, Phase::Verification, "s.yaml").unwrap();
                assert_eq!(
                    result.diagnostics[0].context["count"],
                    Fact::Unsigned(usize::MAX)
                );
                assert_eq!(
                    result.diagnostics[0].context["keys"],
                    Fact::Sequence(vec![Fact::Object(Default::default())])
                );
            }
            /// Seam: the projection's `InvalidObservation` refusal.
            #[test]
            fn later_invalid_geometry_refuses_the_entire_projection() {
                let compiled = compiled();
                let good = [Row(vec![Value::Int(1)])];
                let bad = [Row(vec![Value::Int(1), Value::Int(2)])];
                let records = [
                    record(
                        ConditionCode::VerificationAssertFailed,
                        "verifications[2].assert",
                        &good,
                    ),
                    record(
                        ConditionCode::VerificationAllOrNoneFailed,
                        "verifications[1].all_or_none",
                        &bad,
                    ),
                ];
                assert!(matches!(
                    project(&compiled, &records, Phase::Verification, "s.yaml"),
                    Err(Error::InvalidObservation)
                ));
            }
            /// Seam: the projection's `UnsupportedOutcome` refusal.
            #[test]
            fn typed_wrong_family_or_phase_refuses_the_entire_projection() {
                let compiled = compiled();
                let rows = [Row(vec![Value::Int(1)])];
                for (code, path) in [
                    (ConditionCode::ConversionInput, "verifications[2].assert"),
                    (ConditionCode::OutputMissingKeyFailed, "keys[0]"),
                ] {
                    let records = [
                        record(
                            ConditionCode::VerificationAssertFailed,
                            "verifications[2].assert",
                            &rows,
                        ),
                        record(code, path, &rows),
                    ];
                    assert!(matches!(
                        project(&compiled, &records, Phase::Verification, "s.yaml"),
                        Err(Error::UnsupportedOutcome)
                    ));
                }
            }
        }
        mod yaml_source {
            use yamaa_core::{
                project_diagnostics::Fact,
                yaml_diagnostics::{diagnostics, Failure, UnicodeIssue},
            };
            /// Seam: facts borrowed by address, and an offset of `usize::MAX`.
            #[test]
            fn reason_and_identity_borrow_original_facts_and_unicode_order_is_complete() {
                let reason = String::from("retained parser reason");
                let identity = String::from("source.yaml");
                let result = diagnostics(
                    Failure::InvalidYaml {
                        line: 1,
                        column: 2,
                        reason: &reason,
                    },
                    &identity,
                );
                let Fact::Text(held) = result[0].context["reason"] else {
                    panic!("borrowed reason")
                };
                assert!(std::ptr::eq(held.as_ptr(), reason.as_ptr()));
                let result = diagnostics(Failure::NonAscii { line: 1, column: 2 }, &identity);
                let Fact::Text(held) = result[0].context["path"] else {
                    panic!("borrowed source identity")
                };
                assert!(std::ptr::eq(held.as_ptr(), identity.as_ptr()));
                let issues = [
                    UnicodeIssue {
                        path: "$.first".into(),
                        code_point: 0xD800,
                        offset: 0,
                    },
                    UnicodeIssue {
                        path: "$.second[2]".into(),
                        code_point: 0xDFFF,
                        offset: usize::MAX,
                    },
                ];
                let result = diagnostics(Failure::InvalidText(&issues), &identity);
                // Every invalid scalar names the file it is read from, borrowed.
                for diagnostic in &result {
                    let Fact::Text(held) = diagnostic.context["path"] else {
                        panic!("borrowed source identity")
                    };
                    assert!(std::ptr::eq(held.as_ptr(), identity.as_ptr()));
                }
                assert_eq!(
                    result
                        .iter()
                        .map(|d| d.spec_path.as_str())
                        .collect::<Vec<_>>(),
                    ["$.first", "$.second[2]"]
                );
                assert_eq!(
                    result[0].context["code_point"],
                    Fact::OwnedText("U+D800".into())
                );
                assert_eq!(result[1].context["offset"], Fact::Unsigned(usize::MAX));
            }
        }
    }
}
