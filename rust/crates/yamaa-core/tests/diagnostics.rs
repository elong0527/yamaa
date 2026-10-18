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
            evaluation::{EvaluationErrorKind, NumericResolver},
            numeric_compiler::compile_numeric,
            numeric_parser::SourceSpan,
            value::{ColumnType, Selection, Value},
        };

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
            for (stage, code) in [
                ("capture", "resource_limit"),
                ("prepare", "compiler_limit"),
                ("check", "compiler_limit"),
                ("ingest", "source_limit"),
                ("bind", "source_limit"),
                ("execute", "execution_limit"),
                ("build", "source_boundary"),
                ("build", "execution_boundary"),
                ("output", "resource_limit"),
            ] {
                let finding = yamaa_core::application_issue::resource_rejected(
                    stage,
                    code,
                    "bytes",
                    7,
                    Some(8),
                    vec!["input.SRC".into()],
                );
                assert_eq!(finding.definition().condition, "engine_rejected");
                assert_eq!(finding.definition().requirement, None);
                assert_eq!(finding.spec_paths, ["input.SRC"]);
                assert_eq!(
                    finding.context,
                    [
                        ("stage".into(), JsonValue::String(stage.into())),
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
                "execute",
                "execution_limit",
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
                yamaa_core::application_issue::rejected("capture", "changed").context,
                [
                    ("stage".into(), JsonValue::String("capture".into())),
                    ("code".into(), JsonValue::String("changed".into())),
                ]
                .into()
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
                    publication.definition().requirement
                ),
                ("output", "publication_failed", Some("REQ-0764"))
            );
            assert_eq!(publication.spec_paths, ["output.path"]);
            assert_eq!(
                publication.context["path"],
                JsonValue::String("generated/producer.csv".into())
            );
            reached.insert(publication.code);
            for (finding, phase, condition, context) in [
                (
                    yamaa_core::application_issue::unsupported("environment", Some("environment")),
                    "planning",
                    "unsupported_operation",
                    [("operation", "environment")].as_slice(),
                ),
                (
                    yamaa_core::application_issue::rejected("capture", "changed"),
                    "engine",
                    "engine_rejected",
                    [("code", "changed"), ("stage", "capture")].as_slice(),
                ),
            ] {
                let definition = finding.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    (phase, condition, None)
                );
                for &(name, value) in context {
                    assert_eq!(finding.context[name], JsonValue::String(value.into()));
                }
                reached.insert(finding.code);
            }
            for (expression, phase, condition, requirement) in [
                ("MISSING", "validation", "unknown_field", "REQ-0443"),
                ("TEXT", "validation", "incompatible_input_type", "REQ-0444"),
                (
                    "ROUND_HALF_AWAY_FROM_ZERO(1, 1.0)",
                    "validation",
                    "incompatible_input_type",
                    "REQ-0418",
                ),
                (
                    "9223372036854775807 + 1",
                    "derivation",
                    "integer_overflow",
                    "REQ-0434",
                ),
                ("1 / 0", "derivation", "division_by_zero", "REQ-0430"),
                ("SQRT(-1)", "derivation", "sqrt_of_negative", "REQ-0431"),
                ("LN(0)", "derivation", "ln_of_nonpositive", "REQ-0432"),
                ("POWER(-1, 0.5)", "derivation", "invalid_power", "REQ-0433"),
            ] {
                let plan = compile_numeric(expression, "columns.X.compute").unwrap();
                let error = plan.evaluate(&mut Bindings).unwrap_err();
                let diagnostic = error.diagnostic().unwrap();
                let definition = diagnostic.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    (phase, condition, Some(requirement))
                );
                assert_eq!(diagnostic.spec_paths, ["columns.X.compute"]);
                assert_eq!(
                    diagnostic.context["expr"],
                    JsonValue::String(expression.into())
                );
                reached.insert(diagnostic.code);
            }
            for (source, target, requirement) in [
                (Value::Bool(true), ColumnType::Int, "REQ-0013"),
                (Value::float(1.5), ColumnType::Int, "REQ-0021"),
                (
                    Value::Str("2026-02-30".into()),
                    ColumnType::Date,
                    "REQ-0601",
                ),
            ] {
                let error = convert(&source, target).unwrap_err();
                let diagnostic = error.into_diagnostic("columns.X".into());
                let definition = diagnostic.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    ("convert", "conversion_failed", Some(requirement))
                );
                assert_eq!(diagnostic.context["value"], JsonValue::from(source));
                assert_eq!(diagnostic.spec_paths, ["columns.X"]);
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
                reached.insert(diagnostic.code);
            }
            for (cause, condition, requirement) in [
                (
                    yamaa_core::resource::ResourceFailure::Missing,
                    "resource_path_missing",
                    "REQ-0785",
                ),
                (
                    yamaa_core::resource::ResourceFailure::NotRegularFile,
                    "resource_path_not_regular_file",
                    "REQ-0785",
                ),
                (
                    yamaa_core::resource::ResourceFailure::UriScheme,
                    "resource_path_uri_scheme",
                    "REQ-0775",
                ),
                (
                    yamaa_core::resource::ResourceFailure::Backslash,
                    "resource_path_not_normalized",
                    "REQ-0776",
                ),
                (
                    yamaa_core::resource::ResourceFailure::EmptySegment,
                    "resource_path_not_normalized",
                    "REQ-0777",
                ),
                (
                    yamaa_core::resource::ResourceFailure::Symlink,
                    "resource_path_symlink",
                    "REQ-0783",
                ),
            ] {
                let diagnostic = cause.diagnostic("SRC", "input/data.csv");
                let definition = diagnostic.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    ("validation", condition, Some(requirement))
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
                reached.insert(diagnostic.code);
            }
            let changed = yamaa_core::resource::ResourceFailure::ContentChanged
                .diagnostic("SRC", "../authored/data.csv");
            assert_eq!(changed.definition().phase, "ingest");
            assert_eq!(
                changed.definition().condition,
                "resource_path_content_changed"
            );
            assert_eq!(changed.definition().requirement, Some("REQ-0788"));
            assert_eq!(changed.spec_paths, ["input.SRC.path"]);
            assert_eq!(
                changed.context["path"],
                JsonValue::String("../authored/data.csv".into())
            );
            assert_eq!(changed.context["dataset"], JsonValue::String("SRC".into()));
            reached.insert(changed.code);
            reached.extend(submission::reached());
            reached.extend(schema::reached());
            reached.extend(yaml_source::reached());
            reached.extend(project_activation::reached());
            reached.extend(project_functions::reached());
            reached.extend(project_environment::reached());
            reached.extend(project_lock::reached());
            reached.extend(project_scope::reached());
            let lock = yamaa_core::application_issue::lock_mismatch(
                [("package".into(), JsonValue::String("project".into()))].into(),
            );
            assert_eq!(
                (
                    lock.definition().phase,
                    lock.definition().condition,
                    lock.definition().requirement
                ),
                ("validation", "runtime_artifact_mismatch", Some("REQ-0697"))
            );
            assert_eq!(lock.spec_paths, ["lock"]);
            assert_eq!(lock.context["package"], JsonValue::String("project".into()));
            reached.insert(lock.code);
            reached.extend(project_codelist_binding::reached());
            reached.extend(preflight::reached());
            reached.extend(preflight::self_reached());
            reached.extend(preflight::intermediate_reached());
            reached.extend(preflight::namespace_reached());
            reached.extend(binding::intermediate_key_reached());
            reached.extend(intermediate_ranges::reached());
            reached.extend(producer::reached());
            reached.extend(preflight::output_reached());
            reached.extend(grammar::reached());
            reached.extend(inheritance::reached());
            reached.extend(grammar::predicate_reached());
            reached.extend(predicate_evaluation::reached());
            reached.extend(binding::reached());
            reached.extend(intermediate_scopes::reached());
            reached.extend(binding::window_reached());
            reached.extend(binding::lookup_reached());
            reached.extend(binding::source_filter_reached());
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
                    diagnostic.definition().requirement
                ),
                ("verification", "not_missing_failed", Some("REQ-0375"))
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
            reached.insert(diagnostic.code);
            for (check, values, offending, condition, requirement) in [
                (
                    yamaa_core::dataset::Check::AllowedValues(vec![Value::Int(1)]),
                    vec![Value::Int(1), Value::Missing, Value::Int(2)],
                    vec![2],
                    "allowed_values_failed",
                    "REQ-0376",
                ),
                (
                    yamaa_core::dataset::Check::Range {
                        min: Some(Value::float(9223372036854775808.0)),
                        max: None,
                    },
                    vec![Value::Int(i64::MAX), Value::Missing],
                    vec![0],
                    "range_failed",
                    "REQ-0377",
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
                    "REQ-0378",
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
                        diagnostic.definition().requirement
                    ),
                    ("verification", condition, Some(requirement))
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
                assert!(yamaa_core::dataset_checks::column_diagnostic(
                    &check,
                    "unused".into(),
                    "V".into(),
                    0
                )
                .is_none());
                reached.insert(diagnostic.code);
            }
            let pattern = yamaa_core::regex::Pattern::compile("b").unwrap();
            let rows = yamaa_core::dataset_checks::matches_offenders(
                &pattern,
                &[
                    Value::Str("abc".into()),
                    Value::Missing,
                    Value::Str("xxx".into()),
                ],
            )
            .unwrap();
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
                    diagnostic.definition().requirement
                ),
                ("verification", "matches_failed", Some("REQ-0379"))
            );
            assert_eq!(diagnostic.context.len(), 2);
            assert_eq!(
                diagnostic.context["failure_count"],
                JsonValue::Number(("1").parse().expect("canonical integer"))
            );
            reached.insert(diagnostic.code);
            assert!(matches!(
                yamaa_core::regex::Pattern::compile("("),
                Err(yamaa_core::regex::CompileError::Invalid { .. })
            ));
            let diagnostic = yamaa_core::dataset_checks::invalid_pattern_diagnostic(
                "columns.V.verifications[0].matches.pattern".into(),
                "(".into(),
            );
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().requirement
                ),
                ("validation", "invalid_regex", Some("REQ-0827"))
            );
            assert_eq!(diagnostic.context.len(), 1);
            assert_eq!(diagnostic.context["pattern"], JsonValue::String("(".into()));
            assert_eq!(diagnostic.source_span, None);
            reached.insert(diagnostic.code);
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
                    diagnostic.definition().requirement
                ),
                ("validation", "incompatible_input_type", Some("REQ-0418"))
            );
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.RND.derivation.round_half_away_from_zero.source"]
            );
            assert_eq!(diagnostic.context.len(), 3);
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            reached.insert(diagnostic.code);
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
                    finding.definition().requirement
                ),
                ("verification", "allowed_values_failed", Some("REQ-0957"))
            );
            assert_eq!(finding.context["codelist"], JsonValue::String("SEX".into()));
            reached.insert(finding.code);
            let extreme = yamaa_core::scalar_language::incomparable_diagnostic(
                &["DTHDT".into(), "DAY".into()],
                &[ColumnType::Date, ColumnType::Int],
                "columns.RESULT.derivation.greatest",
            );
            assert_eq!(
                (
                    extreme.definition().phase,
                    extreme.definition().condition,
                    extreme.definition().requirement
                ),
                ("validation", "incomparable_sources", Some("REQ-0324"))
            );
            assert_eq!(extreme.spec_paths, ["columns.RESULT.derivation.greatest"]);
            assert_eq!(
                extreme.context["types"],
                JsonValue::Array(vec![
                    JsonValue::String("date".into()),
                    JsonValue::String("int".into())
                ])
            );
            reached.insert(extreme.code);
            assert_eq!(reached, CONDITIONS.iter().copied().collect());
            assert_eq!(
                CONDITIONS.len(),
                reached.len(),
                "registry causes are unique"
            );
        }

        #[test]
        fn column_pattern_searches_reuse_the_plan_after_a_fixed_work_refusal() {
            use yamaa_core::{dataset_checks::matches_offenders, regex};
            let pattern = regex::Pattern::compile("z").unwrap();
            let values = [Value::Str("é".into()), Value::Missing];
            for _ in 0..3 {
                assert_eq!(matches_offenders(&pattern, &values).unwrap(), [0]);
            }
            let long = [Value::Str("é".repeat(1_000_000))];
            assert_eq!(
                matches_offenders(&pattern, &long),
                Err(regex::MatchError {
                    resource: regex::Resource::Work,
                    limit: 1_000_000,
                })
            );
            assert_eq!(matches_offenders(&pattern, &values).unwrap(), [0]);
        }

        /// A later nested error keeps its own span/route and full integer diagnostic,
        /// independently of the surrounding successful operand and parentheses.
        #[test]
        fn nested_failure_retains_location_and_full_width_context() {
            let expression = "1 + (9223372036854775807 + 1)";
            let error = compile_numeric(expression, "columns.X.compute")
                .unwrap()
                .evaluate(&mut Bindings)
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
            let literal = compile_numeric("9223372036854775808", "columns.Y.compute")
                .unwrap()
                .evaluate(&mut Bindings)
                .unwrap_err()
                .diagnostic()
                .unwrap();
            assert_eq!(literal.code, diagnostic.code);
            assert_eq!(literal.context["value"], diagnostic.context["value"]);
        }

        #[test]
        fn conversion_retains_unbounded_diagnostic_integer_as_decimal_text() {
            let digits = "9".repeat(5000);
            let diagnostic = convert(&Value::Str(digits.clone()), ColumnType::Int)
                .unwrap_err()
                .into_diagnostic("columns.X.unconvertible".into());
            assert_eq!(diagnostic.definition().requirement, Some("REQ-0021"));
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
                .evaluate(&mut resolver)
                .unwrap_err();
            assert!(error.diagnostic().is_none());
            assert_eq!(resolver.calls, ["BAD"]);
            let EvaluationErrorKind::Resolution {
                identifier,
                error: Failure(value),
            } = error.evaluation.kind
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
