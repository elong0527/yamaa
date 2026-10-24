//! Canonical diagnostic vocabulary and complete native assertion families.
mod diagnostics {
    mod diagnostic {
        use crate::support::diagnostic::*;
        use std::convert::Infallible;
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            conversion::convert,
            diagnostic::CONDITIONS,
            expression::{
                compile, CompileError, CompiledExpression, Context,
                ErrorKind as EvaluationErrorKind, Resolver as NumericResolver, SourceSpan,
            },
            value::{ColumnType, Selection, Value},
        };

        fn compile_numeric(text: &str, path: &str) -> Result<CompiledExpression, CompileError> {
            compile(text, Context::Derivation, path)
        }

        struct Bindings;
        impl NumericResolver for Bindings {
            type Error = Infallible;
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                Ok(if name == "TEXT" {
                    Selection::Present(Value::Str("not numeric".into()))
                } else {
                    Selection::Absent
                })
            }
        }

        #[test]
        fn fixed_refusals_preserve_exact_resource_facts_and_authored_paths() {
            use yamaa_core::application_issue::{Fault, Step};
            for (stage, fault, code) in [
                (Step::Capture, Fault::ResourceLimit, "resource_limit"),
                (Step::Prepare, Fault::CompilerLimit, "compiler_limit"),
                (Step::Check, Fault::CompilerLimit, "compiler_limit"),
                (Step::Ingest, Fault::SourceLimit, "source_limit"),
                (Step::Bind, Fault::SourceLimit, "source_limit"),
                (Step::Execute, Fault::ExecutionLimit, "execution_limit"),
                (Step::Build, Fault::SourceBoundary, "source_boundary"),
                (Step::Build, Fault::ExecutionBoundary, "execution_boundary"),
                (Step::Output, Fault::ResourceLimit, "resource_limit"),
            ] {
                let finding = yamaa_core::application_issue::resource_rejected(
                    stage,
                    fault,
                    "bytes",
                    7,
                    Some(8),
                    vec!["input.SRC".into()],
                );
                assert_eq!(finding.definition().condition, "engine_rejected");
                assert_eq!(finding.definition().rule, None);
                assert_eq!(finding.spec_paths, ["input.SRC"]);
                assert_eq!(
                    finding.context,
                    [
                        ("step".into(), JsonValue::String(stage.as_str().into())),
                        ("code".into(), JsonValue::String(code.into())),
                        ("resource".into(), JsonValue::String("bytes".into())),
                        ("limit".into(), JsonValue::from(7)),
                        ("actual".into(), JsonValue::from(8)),
                    ]
                    .into()
                );
                assert_eq!(finding.source_span, None);
                assert_eq!(finding.operand_route, None);
            }
            let finding = yamaa_core::application_issue::resource_rejected(
                Step::Execute,
                Fault::ExecutionLimit,
                "table_cells",
                640_000_000,
                None,
                vec!["columns".into()],
            );
            assert_eq!(
                finding.context,
                [
                    ("step".into(), JsonValue::String("execute".into())),
                    ("code".into(), JsonValue::String("execution_limit".into())),
                    ("resource".into(), JsonValue::String("table_cells".into())),
                    ("limit".into(), JsonValue::from(640_000_000)),
                ]
                .into()
            );
            assert_eq!(finding.spec_paths, ["columns"]);
            assert_eq!(
                yamaa_core::application_issue::rejected(Step::Capture, Fault::ResourceChanged)
                    .context,
                [
                    ("step".into(), JsonValue::String("capture".into())),
                    ("code".into(), JsonValue::String("resource_changed".into())),
                ]
                .into()
            );
        }

        /// The engine fault vocabulary is public context: every `step` and
        /// `code` spelling is frozen here, in declaration order, so renaming a
        /// variant cannot move what a refusal reports.
        #[test]
        fn engine_fault_stages_and_codes_keep_their_spellings() {
            use yamaa_core::application_issue::{Fault, Step};
            assert_eq!(
                Step::ALL
                    .iter()
                    .map(|stage| stage.as_str())
                    .collect::<Vec<_>>(),
                [
                    "bind",
                    "build",
                    "capture",
                    "check",
                    "define",
                    "dictionary",
                    "environment",
                    "execute",
                    "ingest",
                    "inheritance",
                    "output",
                    "prepare",
                    "project",
                    "publication_target_admission",
                    "report",
                    "resolve",
                ]
            );
            assert_eq!(
                Fault::ALL
                    .iter()
                    .map(|fault| fault.as_str())
                    .collect::<Vec<_>>(),
                [
                    "activation_boundary",
                    "artifact_representation",
                    "cleanup_incomplete",
                    "compiler_limit",
                    "csv_limit",
                    "declaring_origin",
                    "dependencies",
                    "dependency_policy",
                    "diagnostic_context",
                    "document",
                    "environment_boundary",
                    "execution_boundary",
                    "execution_limit",
                    "host_publication_failure",
                    "host_resource_failure",
                    "inheritance_required",
                    "internal",
                    "invalid_target",
                    "inventory",
                    "issue_projection",
                    "normalization",
                    "numeric_policy",
                    "odm_projection_boundary",
                    "odm_reader_unavailable",
                    "opaque_host",
                    "output_limit",
                    "output_projection",
                    "output_publication_failed",
                    "output_schema",
                    "parent_required",
                    "parquet_boundary",
                    "parquet_codec_unavailable",
                    "parquet_limit",
                    "parquet_reader_unavailable",
                    "partition_global",
                    "path_mismatch",
                    "producer_contract",
                    "producer_metadata",
                    "project_sections",
                    "publication_cancelled",
                    "publication_failed",
                    "publication_limit",
                    "publication_report_limit",
                    "publication_unwind",
                    "reference_catalog",
                    "report_stack",
                    "resource_base",
                    "resource_changed",
                    "resource_limit",
                    "resource_link",
                    "resource_missing",
                    "resource_not_regular",
                    "resource_outside_roots",
                    "resource_path",
                    "resource_root",
                    "resource_unavailable",
                    "resource_unreadable",
                    "result_projection",
                    "sas_boundary",
                    "sas_limit",
                    "sas_reader_unavailable",
                    "schema_bundle",
                    "shipped_schema",
                    "source_boundary",
                    "source_collection_boundary",
                    "source_count",
                    "source_limit",
                    "source_ordinal_boundary",
                    "staging_collision",
                    "table_limit",
                    "target_changed",
                    "transport",
                    "validation_policy",
                    "yaml_admission",
                    "yaml_decode",
                    "yaml_reader_unavailable",
                ]
            );
        }

        /// Every registry entry must be reached by its evaluator, converter or resource cause. Expected
        /// vocabulary is independent literal truth, not copied from the registry at runtime.
        #[test]
        fn every_registered_cause_is_reached_with_its_normative_mapping() {
            // The expected issues written as data, under tests/fixtures/diagnostics/.
            let mut reached = replay::replay();
            // The tables still written in Rust, one group for each part of
            // #3691 that moves them, so the parts never edit the same lines.

            assert_eq!(reached, CONDITIONS.iter().copied().collect());
            assert_eq!(
                CONDITIONS.len(),
                reached.len(),
                "registry causes are unique"
            );
            // Every cause renders its registered message from a record it
            // made, with each placeholder filled from that record (P-MSG-1).
            let rendered = rendered_causes();
            let unrendered = CONDITIONS
                .iter()
                .filter(|code| !rendered.contains(code))
                .collect::<Vec<_>>();
            assert!(
                unrendered.is_empty(),
                "causes reached without rendering a message: {unrendered:?}"
            );
        }

        /// A composition refusal of an unsupported operation names the
        /// operation, as every such refusal does (COLUMN-15), so its message
        /// can say which.
        #[test]
        fn composition_refusals_name_the_unsupported_operation() {
            let finding = yamaa_core::submission_composition::Finding::unsupported(
                "datasets.DM.artifact",
                "published_dataset_format",
            );
            assert_eq!(
                finding.code,
                yamaa_core::diagnostic::ConditionCode::ApplicationUnsupported
            );
            assert_eq!(
                finding.context,
                [(
                    "operation".into(),
                    JsonValue::String("published_dataset_format".into())
                )]
                .into()
            );
            assert_eq!(
                finding.record().message(),
                "published_dataset_format is not supported by this engine"
            );
        }

        /// A column check fails, in row order, each row whose present value
        /// it does not admit, and a missing value only where a value is
        /// required (CHECK-5 to CHECK-9, SUB-11, SUB-26, CT-7). The finding
        /// each check reports is a case of `fixtures/diagnostics/registry.json`.
        #[test]
        fn column_checks_fail_the_rows_they_do_not_admit_in_row_order() {
            use yamaa_core::{
                dataset::Check,
                dataset_checks::{column_offenders, matches_offenders, not_missing},
                iso8601::Form,
                portable_regex::Pattern,
            };
            assert_eq!(
                not_missing(&[
                    Value::Int(0),
                    Value::Missing,
                    Value::Str(String::new()),
                    Value::Missing,
                ])
                .offending_rows,
                [1, 3],
                "not_missing"
            );
            for (check, values, offending, label) in [
                (
                    Check::Mandatory(None),
                    vec![Value::Int(1), Value::Missing, Value::Str(String::new())],
                    vec![1],
                    "mandatory",
                ),
                (
                    Check::AllowedValues(vec![Value::Int(1)]),
                    vec![Value::Int(1), Value::Missing, Value::Int(2)],
                    vec![2],
                    "allowed_values",
                ),
                (
                    Check::Range {
                        min: Some(Value::float(9223372036854775808.0)),
                        max: None,
                    },
                    vec![Value::Int(i64::MAX), Value::Missing],
                    vec![0],
                    "range",
                ),
                (
                    Check::MaxLength(2),
                    vec![
                        Value::Str("é🙂".into()),
                        Value::Str("é🙂x".into()),
                        Value::Missing,
                    ],
                    vec![1],
                    "max_length",
                ),
                (
                    Check::DataType {
                        form: Form::Date,
                        scope: None,
                    },
                    vec![
                        Value::Str("2025-03-04".into()),
                        Value::Str("2025-03".into()),
                        Value::Missing,
                    ],
                    vec![1],
                    "data_type",
                ),
                (
                    Check::Codelist {
                        id: "SEX".into(),
                        values: vec![Value::Str("F".into())],
                    },
                    vec![Value::Missing, Value::Str("X".into())],
                    vec![1],
                    "codelist",
                ),
            ] {
                assert_eq!(column_offenders(&check, &values), offending, "{label}");
            }
            let pattern = Pattern::compile("b").unwrap();
            assert_eq!(
                matches_offenders(
                    &pattern,
                    &[
                        Value::Str("abc".into()),
                        Value::Missing,
                        Value::Str("xxx".into()),
                    ],
                ),
                [2],
                "matches"
            );
        }

        /// A value-level finding names its column in its message, and the
        /// finding of an entry that selects no row names the entry too
        /// (SUB-13); each check holds its entries' where clauses. The
        /// records are cases of `fixtures/diagnostics/registry.json`, whose
        /// rows hold no message.
        #[test]
        fn value_level_findings_name_their_column_and_entry_in_their_messages() {
            use yamaa_core::dataset::{Check, WhereClause, WhereTerm};
            let clause = |value: &str| WhereClause {
                terms: vec![WhereTerm::listed(1, vec![value.into()])],
            };
            for (check, path, message) in [
                (
                    Check::ValueEntries(vec![("A".into(), clause("X")), ("B".into(), clause("X"))]),
                    "columns.V.submission.value_level",
                    "several value-level entries of column V select the same row in 1 of the rows",
                ),
                (
                    Check::ValueEntryMatch {
                        id: "A".into(),
                        clause: clause("X"),
                    },
                    "columns.V.submission.value_level.A",
                    "value-level entry A of column V selects no row",
                ),
            ] {
                assert!(
                    check
                        .clauses()
                        .iter()
                        .all(|clause| clause.columns().eq([1])),
                    "{path} holds the clauses of its entries"
                );
                let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
                    &check,
                    path.into(),
                    "V".into(),
                    1,
                )
                .unwrap();
                assert_eq!(diagnostic.message(), message, "{path}");
            }
        }

        #[test]
        fn column_pattern_searches_reuse_the_plan_without_a_work_budget() {
            use yamaa_core::{dataset_checks::matches_offenders, portable_regex::Pattern};
            let pattern = Pattern::compile("z").unwrap();
            let values = [Value::Str("é".into()), Value::Missing];
            for _ in 0..3 {
                assert_eq!(matches_offenders(&pattern, &values), [0]);
            }
            // A million-scalar subject, which the retired step budget refused,
            // is searched to completion and simply has no match.
            let long = [Value::Str("é".repeat(1_000_000))];
            assert_eq!(matches_offenders(&pattern, &long), [0]);
            let catastrophic = Pattern::compile("(a+)+b").unwrap();
            assert_eq!(
                matches_offenders(&catastrophic, &[Value::Str("a".repeat(100_000))]),
                [0]
            );
            assert_eq!(matches_offenders(&pattern, &values), [0]);
        }

        /// A later nested error keeps its own span/route and full integer diagnostic,
        /// independently of the surrounding successful operand and parentheses.
        #[test]
        fn nested_failure_retains_location_and_full_width_context() {
            let expression = "1 + (9223372036854775807 + 1)";
            let error = compile_numeric(expression, "columns.X")
                .unwrap()
                .evaluate_number(&mut Bindings)
                .unwrap_err();
            let diagnostic = error.diagnostic().unwrap();
            assert_eq!(
                diagnostic.source_span,
                Some(SourceSpan { start: 5, end: 28 })
            );
            assert_eq!(diagnostic.operand_route, Some(vec!["right".into()]));
            assert_eq!(
                diagnostic.context["value"],
                JsonValue::String("9223372036854775808".into())
            );
            assert_eq!(diagnostic.context["minimum"], JsonValue::from(i64::MIN));
            assert_eq!(diagnostic.context["maximum"], JsonValue::from(i64::MAX));
            // A literal outside the int range is refused before any value is read.
            let Err(CompileError::Invalid {
                position, failure, ..
            }) = compile_numeric("9223372036854775808", "columns.Y")
            else {
                panic!("an overflowing literal is refused")
            };
            let literal = failure.diagnostic(
                Context::Derivation,
                "columns.Y",
                "9223372036854775808",
                position.character,
            );
            assert_eq!(literal.definition().phase, "validation");
            assert_eq!(literal.definition().condition, "integer_overflow");
            assert_eq!(literal.definition().rule, Some("VALUE-5"));
        }

        #[test]
        fn conversion_retains_unbounded_diagnostic_integer_as_decimal_text() {
            let digits = "9".repeat(5000);
            let diagnostic = convert(&Value::Str(digits.clone()), ColumnType::Int)
                .unwrap_err()
                .into_diagnostic("columns.X.unconvertible".into());
            assert_eq!(diagnostic.definition().rule, Some("VALUE-7"));
            assert_eq!(diagnostic.context["from"], JsonValue::String("int".into()));
            assert_eq!(diagnostic.context["to"], JsonValue::String("int".into()));
            assert_eq!(
                diagnostic.context["value"],
                JsonValue::Number((digits).parse().expect("canonical integer"))
            );
        }

        /// The diagnostic projection cannot consume or relabel a host error, and has no
        /// Clone requirement on it. The next operand remains unevaluated.
        #[test]
        fn opaque_resolver_failure_is_retained_in_place() {
            #[derive(Debug)]
            struct Failure(Box<u8>);
            struct Resolver {
                failure: Option<Failure>,
                calls: Vec<String>,
            }
            impl NumericResolver for Resolver {
                type Error = Failure;
                fn resolve(&mut self, name: &str) -> Result<Selection, Failure> {
                    self.calls.push(name.into());
                    Err(self.failure.take().expect("only one call"))
                }
            }
            let failure = Failure(Box::new(9));
            let address = &*failure.0 as *const u8;
            let mut resolver = Resolver {
                failure: Some(failure),
                calls: vec![],
            };
            let error = compile_numeric("BAD + LATER", "columns.X")
                .unwrap()
                .evaluate_number(&mut resolver)
                .unwrap_err();
            assert!(error.diagnostic().is_err());
            assert_eq!(resolver.calls, ["BAD"]);
            let EvaluationErrorKind::Resolution {
                identifier,
                error: Failure(value),
            } = error.kind
            else {
                panic!("opaque failure replaced")
            };
            assert_eq!(identifier, "BAD");
            assert_eq!(&*value as *const u8, address);
        }

        #[test]
        fn scalar_json_context_preserves_typed_kinds_and_exact_number_spellings() {
            let context = std::collections::BTreeMap::from([
                ("bool", yamaa_core::json::Value::from(Value::Bool(true))),
                (
                    "date",
                    yamaa_core::json::Value::from(Value::Date("2025-01-02".parse().unwrap())),
                ),
                (
                    "datetime",
                    yamaa_core::json::Value::from(Value::DateTime(
                        "2025-01-02T03:04".parse().unwrap(),
                    )),
                ),
                ("float", yamaa_core::json::Value::from(Value::float(1.0))),
                ("max", yamaa_core::json::Value::from(Value::Int(i64::MAX))),
                ("min", yamaa_core::json::Value::from(Value::Int(i64::MIN))),
                ("missing", yamaa_core::json::Value::from(Value::Missing)),
                ("negzero", yamaa_core::json::Value::from(Value::float(-0.0))),
                (
                    "text",
                    yamaa_core::json::Value::from(Value::Str("001".into())),
                ),
            ]);
            // The exact wire spelling of this tree is asserted by the adapter
            // wire tests; core owns the typed kinds and number tokens.
            assert_eq!(context["bool"], yamaa_core::json::Value::Bool(true));
            assert_eq!(
                context["date"],
                yamaa_core::json::Value::String("2025-01-02".into())
            );
            assert_eq!(
                context["datetime"],
                yamaa_core::json::Value::String("2025-01-02T03:04:00".into())
            );
            assert_eq!(
                context["float"].as_number().unwrap().as_str(),
                "1.0",
                "float token"
            );
            assert_eq!(
                context["max"].as_number().unwrap().as_str(),
                "9223372036854775807"
            );
            assert_eq!(
                context["min"].as_number().unwrap().as_str(),
                "-9223372036854775808"
            );
            assert_eq!(context["missing"], yamaa_core::json::Value::Null);
            assert_eq!(context["negzero"].as_number().unwrap().as_str(), "-0.0");
            assert_eq!(
                context["text"],
                yamaa_core::json::Value::String("001".into())
            );
        }
    }

    /// How the replay compares an authored case with what its operation
    /// reports (#3691).
    mod replay {
        use crate::support::diagnostic::replay::{among, compared};
        use yamaa_core::json::Value;

        /// A case that compares some keys of the context reads a reported
        /// context at those keys alone: another reported key is ignored, and
        /// a named key the report lacks leaves the row unequal to its author's.
        #[test]
        fn compares_reads_a_context_only_at_the_keys_it_names() {
            let row = |context: &[(&str, &str)]| {
                Value::Object(
                    [
                        (
                            "condition".into(),
                            Value::String("undeclared_column".into()),
                        ),
                        (
                            "context".into(),
                            Value::Object(
                                context
                                    .iter()
                                    .map(|(key, value)| {
                                        ((*key).into(), Value::String((*value).into()))
                                    })
                                    .collect(),
                            ),
                        ),
                        ("phase".into(), Value::String("validation".into())),
                    ]
                    .into(),
                )
            };
            let compares = ["condition".to_owned(), "context.column".to_owned()];
            let projected = |context: &[(&str, &str)]| {
                let Value::Object(mut row) = row(context) else {
                    unreachable!("a row is an object")
                };
                row.remove("phase");
                Value::Object(row)
            };
            let authored = projected(&[("column", "VALUE")]);
            for (reported, expected, matches, label) in [
                (
                    row(&[("column", "VALUE"), ("detail", "VALUE")]),
                    projected(&[("column", "VALUE")]),
                    true,
                    "another reported key is ignored",
                ),
                (
                    row(&[("column", "OTHER")]),
                    projected(&[("column", "OTHER")]),
                    false,
                    "the named key is compared",
                ),
                (
                    row(&[("detail", "VALUE")]),
                    projected(&[]),
                    false,
                    "a named key the report lacks",
                ),
            ] {
                let actual = compared(reported, Some(&compares));
                let equal = actual == authored;
                assert_eq!((actual, equal), (expected, matches), "{label}");
            }
        }

        /// An `among` case's rows are some of the issues its operation
        /// reports: each matches a distinct reported row, in any order, and
        /// a row that matches none is named.
        #[test]
        fn among_rows_each_match_a_distinct_reported_row_in_any_order() {
            let row = |condition: &str| {
                Value::Object([("condition".into(), Value::String(condition.into()))].into())
            };
            let reported = [row("a"), row("b"), row("c")];
            for (authored, expected, label) in [
                (vec![row("b")], Ok(vec![1]), "one row among extra rows"),
                (
                    vec![row("c"), row("a")],
                    Ok(vec![2, 0]),
                    "rows in another order",
                ),
                (
                    vec![row("d")],
                    Err(vec![row("d")]),
                    "a row reported nowhere",
                ),
                (
                    vec![row("a"), row("a")],
                    Err(vec![row("a")]),
                    "a row written twice and reported once",
                ),
            ] {
                assert_eq!(among(&authored, &reported), expected, "{label}");
            }
        }
    }
}
