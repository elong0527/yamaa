//! Canonical diagnostic vocabulary and complete native assertion families.
#[allow(dead_code)]
mod support;
mod diagnostics {
    mod diagnostic {
        use crate::support::diagnostic::*;
        use std::{collections::BTreeSet, convert::Infallible};
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
            compile(text, Context::Compute, path)
        }

        struct Bindings;
        use crate::support::project_environment;
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
            use yamaa_core::application_issue::{Fault, Stage};
            for (stage, fault, code) in [
                (Stage::Capture, Fault::ResourceLimit, "resource_limit"),
                (Stage::Prepare, Fault::CompilerLimit, "compiler_limit"),
                (Stage::Check, Fault::CompilerLimit, "compiler_limit"),
                (Stage::Ingest, Fault::SourceLimit, "source_limit"),
                (Stage::Bind, Fault::SourceLimit, "source_limit"),
                (Stage::Execute, Fault::ExecutionLimit, "execution_limit"),
                (Stage::Build, Fault::SourceBoundary, "source_boundary"),
                (Stage::Build, Fault::ExecutionBoundary, "execution_boundary"),
                (Stage::Output, Fault::ResourceLimit, "resource_limit"),
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
                        ("stage".into(), JsonValue::String(stage.as_str().into())),
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
                Stage::Execute,
                Fault::ExecutionLimit,
                "table_cells",
                640_000_000,
                None,
                vec!["columns".into()],
            );
            assert_eq!(
                finding.context,
                [
                    ("stage".into(), JsonValue::String("execute".into())),
                    ("code".into(), JsonValue::String("execution_limit".into())),
                    ("resource".into(), JsonValue::String("table_cells".into())),
                    ("limit".into(), JsonValue::from(640_000_000)),
                ]
                .into()
            );
            assert_eq!(finding.spec_paths, ["columns"]);
            assert_eq!(
                yamaa_core::application_issue::rejected(Stage::Capture, Fault::ResourceChanged)
                    .context,
                [
                    ("stage".into(), JsonValue::String("capture".into())),
                    ("code".into(), JsonValue::String("resource_changed".into())),
                ]
                .into()
            );
        }

        /// The engine fault vocabulary is public context: every `stage` and
        /// `code` spelling is frozen here, in declaration order, so renaming a
        /// variant cannot move what a refusal reports.
        #[test]
        fn engine_fault_stages_and_codes_keep_their_spellings() {
            use yamaa_core::application_issue::{Fault, Stage};
            assert_eq!(
                Stage::ALL
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
                    "invalid_packaging_lock",
                    "invalid_target",
                    "inventory",
                    "issue_projection",
                    "normalization",
                    "numeric_policy",
                    "odm_projection_boundary",
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
                    "unknown_class",
                    "validation_policy",
                    "versioned_root_required",
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
            let mut reached = BTreeSet::new();
            reached.extend(record_operations::reached());
            reached.extend(mapping_cut::reached());
            reached.extend(temporal_arithmetic::reached());
            let publication =
                yamaa_core::application_issue::publication_failed("generated/producer.csv");
            assert_eq!(
                (
                    publication.definition().phase,
                    publication.definition().condition,
                    publication.definition().rule
                ),
                ("output", "publication_failed", Some("OUTPUT-8"))
            );
            assert_eq!(publication.spec_paths, ["output.path"]);
            assert_eq!(
                publication.context["path"],
                JsonValue::String("generated/producer.csv".into())
            );
            reached.insert(rendered(&publication));
            for (finding, phase, condition, context) in [
                (
                    yamaa_core::application_issue::unsupported("environment", Some("environment")),
                    "planning",
                    "unsupported_operation",
                    [("operation", "environment")].as_slice(),
                ),
                (
                    yamaa_core::application_issue::rejected(
                        yamaa_core::application_issue::Stage::Capture,
                        yamaa_core::application_issue::Fault::ResourceChanged,
                    ),
                    "engine",
                    "engine_rejected",
                    [("code", "resource_changed"), ("stage", "capture")].as_slice(),
                ),
            ] {
                let definition = finding.definition();
                assert_eq!(
                    (definition.phase, definition.condition, definition.rule),
                    (phase, condition, None)
                );
                for &(name, value) in context {
                    assert_eq!(finding.context[name], JsonValue::String(value.into()));
                }
                reached.insert(rendered(&finding));
            }
            for (expression, phase, condition, rule) in [
                ("MISSING", "validation", "unknown_field", "FORMULA-9"),
                (
                    "TEXT",
                    "validation",
                    "incompatible_input_type",
                    "FORMULA-12",
                ),
                (
                    "9223372036854775807 + 1",
                    "derivation",
                    "integer_overflow",
                    "VALUE-5",
                ),
                ("1 / 0", "derivation", "division_by_zero", "FORMULA-15"),
                ("SQRT(-1)", "derivation", "sqrt_of_negative", "FORMULA-15"),
                ("LN(0)", "derivation", "ln_of_nonpositive", "FORMULA-15"),
                (
                    "POWER(-1, 0.5)",
                    "derivation",
                    "invalid_power",
                    "FORMULA-15",
                ),
            ] {
                let plan = compile_numeric(expression, "columns.X.compute").unwrap();
                let error = plan.evaluate_number(&mut Bindings).unwrap_err();
                let diagnostic = error.diagnostic().unwrap();
                let definition = diagnostic.definition();
                assert_eq!(
                    (definition.phase, definition.condition, definition.rule),
                    (phase, condition, Some(rule))
                );
                assert_eq!(diagnostic.spec_paths, ["columns.X.compute"]);
                assert_eq!(
                    diagnostic.context["expr"],
                    JsonValue::String(expression.into())
                );
                reached.insert(rendered(&diagnostic));
            }
            for (source, target, rule) in [
                (Value::Bool(true), ColumnType::Int, "VALUE-3"),
                (Value::float(1.5), ColumnType::Int, "VALUE-7"),
                (
                    Value::Str("2026-02-30".into()),
                    ColumnType::Date,
                    "VALUE-15",
                ),
            ] {
                let error = convert(&source, target).unwrap_err();
                let diagnostic = error.into_diagnostic("columns.X".into());
                let definition = diagnostic.definition();
                assert_eq!(
                    (definition.phase, definition.condition, definition.rule),
                    ("convert", "conversion_failed", Some(rule))
                );
                assert_eq!(diagnostic.context["value"], JsonValue::from(source));
                assert_eq!(diagnostic.spec_paths, ["columns.X"]);
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
                reached.insert(rendered(&diagnostic));
            }
            for (cause, condition, rule) in [
                (
                    yamaa_core::resource::ResourceFailure::Missing,
                    "resource_path_missing",
                    "FILE-7",
                ),
                (
                    yamaa_core::resource::ResourceFailure::NotRegularFile,
                    "resource_path_not_regular_file",
                    "FILE-7",
                ),
                (
                    yamaa_core::resource::ResourceFailure::UriScheme,
                    "resource_path_uri_scheme",
                    "FILE-5",
                ),
                (
                    yamaa_core::resource::ResourceFailure::Backslash,
                    "resource_path_not_normalized",
                    "FILE-5",
                ),
                (
                    yamaa_core::resource::ResourceFailure::EmptySegment,
                    "resource_path_not_normalized",
                    "FILE-5",
                ),
                (
                    yamaa_core::resource::ResourceFailure::Symlink,
                    "resource_path_symlink",
                    "FILE-8",
                ),
            ] {
                let diagnostic = cause.diagnostic("SRC", "input/data.csv");
                let definition = diagnostic.definition();
                assert_eq!(
                    (definition.phase, definition.condition, definition.rule),
                    ("validation", condition, Some(rule))
                );
                assert_eq!(diagnostic.spec_paths, ["input.SRC.path"]);
                assert_eq!(
                    diagnostic.context["dataset"],
                    JsonValue::String("SRC".into())
                );
                assert_eq!(
                    diagnostic.context["path"],
                    JsonValue::String("input/data.csv".into())
                );
                assert_eq!(diagnostic.context.len(), 2);
                reached.insert(rendered(&diagnostic));
            }
            let changed = yamaa_core::resource::ResourceFailure::ContentChanged
                .diagnostic("SRC", "../authored/data.csv");
            assert_eq!(changed.definition().phase, "ingest");
            assert_eq!(
                changed.definition().condition,
                "resource_path_content_changed"
            );
            assert_eq!(changed.definition().rule, Some("FILE-10"));
            assert_eq!(changed.spec_paths, ["input.SRC.path"]);
            assert_eq!(
                changed.context["path"],
                JsonValue::String("../authored/data.csv".into())
            );
            assert_eq!(changed.context["dataset"], JsonValue::String("SRC".into()));
            reached.insert(rendered(&changed));
            reached.extend(submission::reached());
            reached.extend(schema::reached());
            reached.extend(yaml_source::reached());
            reached.extend(project_activation::reached());
            reached.extend(project_functions::reached());
            reached.extend(project_functions::donor_keep_reached());
            reached.extend(project_environment::reached());
            reached.extend(project_environment::extended_alias_reached());
            reached.extend(project_environment::format_name_reached());
            reached.extend(project_environment::function_file_reached());
            reached.extend(project_lock::reached());
            reached.extend(project_scope::reached());
            let lock = yamaa_core::application_issue::lock_mismatch(
                "lock",
                [("package".into(), JsonValue::String("project".into()))].into(),
            );
            assert_eq!(
                (
                    lock.definition().phase,
                    lock.definition().condition,
                    lock.definition().rule
                ),
                ("validation", "runtime_artifact_mismatch", Some("FUNC-3"))
            );
            assert_eq!(lock.spec_paths, ["lock"]);
            assert_eq!(lock.context["package"], JsonValue::String("project".into()));
            reached.insert(rendered(&lock));
            reached.extend(project_codelist_binding::reached());
            reached.extend(preflight::reached());
            reached.extend(preflight::self_reached());
            reached.extend(preflight::intermediate_reached());
            reached.extend(preflight::namespace_reached());
            reached.extend(binding::intermediate_key_reached());
            reached.extend(binding::key_record_reached());
            reached.extend(intermediate_ranges::reached());
            reached.extend(intermediate_ranges::match_phase_reached());
            reached.extend(producer::reached());
            reached.extend(copy_entry::reached());
            reached.extend(preflight::output_reached());
            reached.extend(grammar::reached());
            reached.extend(inheritance::reached());
            reached.extend(language::reached());
            reached.extend(front_end::reached());
            reached.extend(front_end::project_binding_reached());
            reached.extend(grammar::predicate_reached());
            reached.extend(predicate_evaluation::reached());
            reached.extend(binding::reached());
            reached.extend(intermediate_scopes::reached());
            reached.extend(binding::window_reached());
            reached.extend(binding::window_driver_reached());
            reached.extend(binding::lookup_reached());
            reached.extend(binding::source_filter_reached());
            reached.extend(binding::source_match_reached());
            reached.extend(binding::source_range_reached());
            reached.extend(binding::implicit_source_reached());
            reached.extend(binding::verification_reached());
            reached.extend(binding::verification_declarations_reached());
            let result = yamaa_core::dataset_checks::not_missing(&[
                Value::Int(0),
                Value::Missing,
                Value::Str(String::new()),
                Value::Missing,
            ]);
            assert_eq!(result.offending_rows, [1, 3]);
            let diagnostic = result
                .diagnostic("columns.V.verifications[0].not_missing".into(), "V".into())
                .unwrap();
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().rule
                ),
                ("verification", "not_missing_failed", Some("CHECK-5"))
            );
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.V.verifications[0].not_missing"]
            );
            assert_eq!(diagnostic.context["column"], JsonValue::String("V".into()));
            assert_eq!(
                diagnostic.context["failure_count"],
                JsonValue::Number(("2").parse().expect("canonical integer"))
            );
            assert!(yamaa_core::dataset_checks::not_missing(&[
                Value::Int(0),
                Value::Str(String::new())
            ])
            .diagnostic("unused".into(), "V".into())
            .is_none());
            reached.insert(rendered(&diagnostic));
            for (check, values, offending, condition, rule) in [
                (
                    yamaa_core::dataset::Check::Mandatory(None),
                    vec![Value::Int(1), Value::Missing, Value::Str(String::new())],
                    vec![1],
                    "not_missing_failed",
                    "SUB-11",
                ),
                (
                    yamaa_core::dataset::Check::AllowedValues(vec![Value::Int(1)]),
                    vec![Value::Int(1), Value::Missing, Value::Int(2)],
                    vec![2],
                    "allowed_values_failed",
                    "CHECK-6",
                ),
                (
                    yamaa_core::dataset::Check::Range {
                        min: Some(Value::float(9223372036854775808.0)),
                        max: None,
                    },
                    vec![Value::Int(i64::MAX), Value::Missing],
                    vec![0],
                    "range_failed",
                    "CHECK-7",
                ),
                (
                    yamaa_core::dataset::Check::MaxLength(2),
                    vec![
                        Value::Str("é🙂".into()),
                        Value::Str("é🙂x".into()),
                        Value::Missing,
                    ],
                    vec![1],
                    "length_failed",
                    "CHECK-8",
                ),
                (
                    yamaa_core::dataset::Check::DataType {
                        form: yamaa_core::iso8601::Form::Date,
                        scope: None,
                    },
                    vec![
                        Value::Str("2025-03-04".into()),
                        Value::Str("2025-03".into()),
                        Value::Missing,
                    ],
                    vec![1],
                    "data_type_failed",
                    "SUB-26",
                ),
            ] {
                let rows = yamaa_core::dataset_checks::column_offenders(&check, &values);
                assert_eq!(rows, offending);
                let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
                    &check,
                    "columns.V.verifications[0]".into(),
                    "V".into(),
                    rows.len(),
                )
                .unwrap();
                assert_eq!(
                    (
                        diagnostic.definition().phase,
                        diagnostic.definition().condition,
                        diagnostic.definition().rule
                    ),
                    ("verification", condition, Some(rule))
                );
                assert_eq!(diagnostic.spec_paths, ["columns.V.verifications[0]"]);
                assert_eq!(diagnostic.context["column"], JsonValue::String("V".into()));
                assert_eq!(
                    diagnostic.context["failure_count"],
                    JsonValue::Number(("1").parse().expect("canonical integer"))
                );
                if condition == "length_failed" {
                    assert_eq!(
                        diagnostic.context["max"],
                        JsonValue::Number(("2").parse().expect("canonical integer"))
                    );
                }
                if condition == "data_type_failed" {
                    assert_eq!(
                        diagnostic.context["data_type"],
                        JsonValue::String("date".into())
                    );
                }
                assert!(yamaa_core::dataset_checks::column_diagnostic(
                    &check,
                    "unused".into(),
                    "V".into(),
                    0
                )
                .is_none());
                reached.insert(rendered(&diagnostic));
            }
            // A row two value-level entries select fails; an entry that
            // selects no row is a warning (SUB-13).
            let clause = |value: &str| yamaa_core::dataset::WhereClause {
                terms: vec![(1, vec![value.into()])],
            };
            for (check, path, condition) in [
                (
                    yamaa_core::dataset::Check::ValueEntries(vec![
                        ("A".into(), clause("X")),
                        ("B".into(), clause("X")),
                    ]),
                    "columns.V.submission.value_level",
                    "value_level_multiple_matches",
                ),
                (
                    yamaa_core::dataset::Check::ValueEntryMatch {
                        id: "A".into(),
                        clause: clause("X"),
                    },
                    "columns.V.submission.value_level.A",
                    "value_level_unmatched",
                ),
            ] {
                assert!(check
                    .clauses()
                    .iter()
                    .all(|clause| clause.columns().eq([1])));
                let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
                    &check,
                    path.into(),
                    "V".into(),
                    1,
                )
                .unwrap();
                assert_eq!(
                    (
                        diagnostic.definition().phase,
                        diagnostic.definition().condition,
                        diagnostic.definition().rule
                    ),
                    ("verification", condition, Some("SUB-13"))
                );
                assert_eq!(diagnostic.spec_paths, [path]);
                reached.insert(rendered(&diagnostic));
            }
            let pattern = yamaa_core::portable_regex::Pattern::compile("b").unwrap();
            let rows = yamaa_core::dataset_checks::matches_offenders(
                &pattern,
                &[
                    Value::Str("abc".into()),
                    Value::Missing,
                    Value::Str("xxx".into()),
                ],
            );
            assert_eq!(rows, [2]);
            let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
                &yamaa_core::dataset::Check::Matches(pattern),
                "columns.V.verifications[0].matches".into(),
                "V".into(),
                rows.len(),
            )
            .unwrap();
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().rule
                ),
                ("verification", "matches_failed", Some("CHECK-9"))
            );
            assert_eq!(diagnostic.context.len(), 2);
            assert_eq!(
                diagnostic.context["failure_count"],
                JsonValue::Number(("1").parse().expect("canonical integer"))
            );
            reached.insert(rendered(&diagnostic));
            assert!(matches!(
                yamaa_core::portable_regex::Pattern::compile("("),
                Err(yamaa_core::portable_regex::CompileError::Invalid { .. })
            ));
            let diagnostic = yamaa_core::dataset_checks::invalid_pattern_diagnostic(
                "columns.V.verifications[0].matches.pattern".into(),
                "(".into(),
            );
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().rule
                ),
                ("validation", "invalid_regex", Some("TEXT-6"))
            );
            assert_eq!(diagnostic.context.len(), 1);
            assert_eq!(diagnostic.context["pattern"], JsonValue::String("(".into()));
            assert_eq!(diagnostic.source_span, None);
            reached.insert(rendered(&diagnostic));
            reached.extend(dataset::reached());
            reached.extend(aggregate::reached());
            reached.extend(verification::reached());
            reached.extend(odm::reached());
            let diagnostic = yamaa_core::dataset::Rounding::new(
                "SEX",
                "columns.RND.derivation.round_half_away_from_zero",
                "1",
            )
            .unwrap()
            .apply(yamaa_core::table::ValueRef::Str("F"))
            .unwrap_err();
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().rule
                ),
                ("validation", "incompatible_input_type", Some("FORMULA-16"))
            );
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.RND.derivation.round_half_away_from_zero.source"]
            );
            assert_eq!(diagnostic.context.len(), 3);
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            reached.insert(rendered(&diagnostic));
            reached.extend(preflight::temporal_reached());
            reached.extend(temporal_imputation::reached());
            reached.extend(text_case::reached());
            reached.extend(text_pad::reached());
            reached.extend(declarative_flags::reached());
            reached.extend(text_extract::reached());
            reached.extend(text_template::reached());
            reached.extend(csv::reached());
            reached.extend(csv::typing_reached());
            reached.extend(parquet::reached());
            reached.extend(sas::reached());
            reached.extend(odm_read::reached());
            reached.extend(result_types::reached());
            let check = yamaa_core::dataset::Check::Codelist {
                id: "SEX".into(),
                values: vec![Value::Str("F".into())],
            };
            assert_eq!(
                yamaa_core::dataset_checks::column_offenders(
                    &check,
                    &[Value::Missing, Value::Str("X".into())]
                ),
                [1]
            );
            let finding = yamaa_core::dataset_checks::column_diagnostic(
                &check,
                "columns.SEX.submission.codelist".into(),
                "SEX".into(),
                1,
            )
            .unwrap();
            assert_eq!(
                (
                    finding.definition().phase,
                    finding.definition().condition,
                    finding.definition().rule
                ),
                ("verification", "allowed_values_failed", Some("CT-7"))
            );
            assert_eq!(finding.context["codelist"], JsonValue::String("SEX".into()));
            reached.insert(rendered(&finding));
            let extreme = yamaa_core::scalar_language::incomparable_diagnostic(
                &["DTHDT".into(), "DAY".into()],
                &[ColumnType::Date, ColumnType::Int],
                "columns.RESULT.derivation.greatest",
            );
            assert_eq!(
                (
                    extreme.definition().phase,
                    extreme.definition().condition,
                    extreme.definition().rule
                ),
                ("validation", "incomparable_sources", Some("VALUE-19"))
            );
            assert_eq!(extreme.spec_paths, ["columns.RESULT.derivation.greatest"]);
            assert_eq!(
                extreme.context["types"],
                JsonValue::Array(vec![
                    JsonValue::String("date".into()),
                    JsonValue::String("int".into())
                ])
            );
            reached.insert(rendered(&extreme));
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
            let error = compile_numeric(expression, "columns.X.compute")
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
            }) = compile_numeric("9223372036854775808", "columns.Y.compute")
            else {
                panic!("an overflowing literal is refused")
            };
            let literal = failure.diagnostic(
                Context::Compute,
                "columns.Y.compute",
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
}
