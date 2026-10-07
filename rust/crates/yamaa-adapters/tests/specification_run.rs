//! Original-document compiler and source-port integration contracts.
use std::{path::Path, sync::Arc};
use yamaa_adapters::{
    arrow_table::TableLimits,
    csv_source,
    specification_source::{CapturedSchema, PreparedDocument, Source},
    yaml_decode::decode_yaml,
};
use yamaa_core::{
    evaluation::EvaluationErrorKind,
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::{TableAccess, TableSchema},
    value::Value,
};
use yamaa_engine::dataset::DatasetExecution;
use yamaa_engine::dataset::{self, DatasetPlan};

fn text(d: &Document, id: usize) -> &str {
    let N::Text(value) = &d.nodes()[id] else {
        panic!("expected text")
    };
    value
}
fn sequence(d: &Document, id: usize) -> &[usize] {
    let N::Sequence(value) = &d.nodes()[id] else {
        panic!("expected sequence")
    };
    value
}
fn schema(root: &Path) -> Arc<CapturedSchema> {
    fn visit(root: &Path, name: &str, modules: &mut Vec<Source>) {
        if modules.iter().any(|m| m.identity == name) {
            return;
        }
        let d = decode_yaml(&std::fs::read(root.join(name)).unwrap(), Default::default())
            .unwrap()
            .document;
        let children = d
            .field(d.root(), "includes")
            .map(|id| {
                sequence(&d, id)
                    .iter()
                    .map(|&id| text(&d, id).to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        modules.push(Source {
            identity: name.into(),
            bytes: std::fs::read(root.join(name)).unwrap(),
        });
        for child in children {
            visit(root, &child, modules);
        }
    }
    let mut modules = Vec::new();
    visit(root, "schema.yaml", &mut modules);
    CapturedSchema::admit(modules, 0, Default::default()).unwrap()
}
fn prepare(schema: &Arc<CapturedSchema>, source: &[u8]) -> PreparedDocument {
    schema
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: source.to_vec(),
        })
        .unwrap()
}
fn lower(spec: &SpecificationDocument, source: &TableSchema) -> DatasetPlan {
    yamaa_engine::specification::PreparedSpecification::prepare(spec)
        .unwrap()
        .bind(source)
        .unwrap()
}
fn limits() -> dataset::Limits {
    dataset::Limits {
        source_rows: 1_000_000,
        output_rows: 1_000_000,
        output_cells: 10_000_000,
        key_cells: 10_000_000,
        work_cells: 100_000_000,
        scalar_text_bytes: 1_000_000,
        output_text_bytes: 10_000_000,
        identity_cells: 10_000_000,
        identity_text_bytes: 10_000_000,
    }
}

#[test]
fn parquet_input_policy_and_owned_source_observations_use_the_shared_build_path() {
    use yamaa_adapters::specification_run::{PreparedRun, SourcePort};
    use yamaa_core::specification::SourceDeclaration;
    use yamaa_core::table::ValueRef;
    struct Port {
        content: Arc<[u8]>,
        calls: usize,
    }
    impl SourcePort for Port {
        type Error = std::convert::Infallible;
        fn capture_reads(&self) -> usize {
            self.calls
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            maximum: usize,
        ) -> Result<Arc<[u8]>, Self::Error> {
            assert_eq!(source.path, "input.PARQUET");
            assert!(self.content.len() <= maximum);
            self.calls += 1;
            Ok(self.content.clone())
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    for (policy, first) in [
        ("missing", ValueRef::Missing),
        ("present", ValueRef::Str("")),
    ] {
        let raw = format!(
            r#"schema_version: "1.0"
domain: TEST
input:
  SRC: {{path: input.PARQUET, empty_string: {policy}}}
keys: [I]
columns:
  - {{name: I, type: int, derivation: SRC.I}}
  - {{name: S, type: str, derivation: SRC.S}}
output: {{path: output.csv, columns: [I, S]}}
"#
        );
        let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
        let content = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pq/text.parquet"),
        )
        .unwrap();
        let mut port = Port {
            content: content.into(),
            calls: 0,
        };
        let attempt = run.execute_with_port(&mut port);
        assert_eq!(port.calls, 1);
        drop(port);
        drop(run);
        let source = attempt.sources[0].table.as_ref().unwrap();
        assert_eq!(source.cell(0, 1).unwrap(), first);
        assert_eq!(source.cell(1, 1).unwrap(), ValueRef::Str("X"));
        assert!(attempt.sources[0].read.captured);
        assert_eq!(attempt.sources[0].read.snapshots_created, Some(1));
        let response = attempt.result.unwrap();
        assert!(response.table.is_some());
    }
}
#[test]
fn original_yaml_to_checked_numeric_failure_without_python_plan() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    for (name, condition, requirement, path, keys) in [
        (
            "negative-zero-division",
            "division_by_zero",
            "REQ-0430",
            "columns.PCHG.derivation.compute",
            vec!["PILOT7", "P7-742", "ALT"],
        ),
        (
            "negative-integer-overflow",
            "integer_overflow",
            "REQ-0434",
            "columns.CELLTOT.derivation.compute",
            vec!["YAMAA-01", "YAMAA-01-102"],
        ),
    ] {
        let case = root.join("benchmarks").join(name);
        let spec = prepare(&schema, &std::fs::read(case.join("spec.yaml")).unwrap());
        let source = csv_source::parse_text_table(
            &std::fs::read(case.join("input/lb.csv")).unwrap(),
            Default::default(),
            TableLimits {
                max_rows: 1_000_000,
                max_columns: 4096,
                max_batches: 1,
                max_cells: 10_000_000,
            },
        )
        .unwrap();
        let plan = lower(spec.model(), source.schema());
        drop(spec);
        let attempt = plan.execute_observed(&source, limits());
        assert!(attempt.handler_counts.is_empty());
        let error = attempt.result.unwrap_err();
        let dataset::ExecutionError::Numeric { error, identity } = *error else {
            panic!("unexpected {error:?}")
        };
        assert_eq!(error.evaluation.location.spec_path, path);
        let EvaluationErrorKind::Numeric(actual) = error.evaluation.kind else {
            panic!("not numeric")
        };
        assert_eq!(actual.condition(), condition);
        assert_eq!(actual.requirement(), requirement);
        assert_eq!(
            identity.unwrap().values,
            keys.into_iter()
                .map(|s| Value::Str(s.into()))
                .collect::<Vec<_>>()
        );
        println!("{name}: {actual:?}");
    }
}

#[test]
fn repeated_keys_collect_raw_readings_before_conversion() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let spec = prepare(
        &schema,
        &std::fs::read(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap(),
    );
    let table_limits = TableLimits {
        max_rows: 100,
        max_columns: 10,
        max_batches: 1,
        max_cells: 1000,
    };
    let same = b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,12,10\nS,P,ALT,12,10\n";
    let table = csv_source::parse_text_table(same, Default::default(), table_limits).unwrap();
    let plan = lower(spec.model(), table.schema());
    let result = plan.execute(&table, limits()).unwrap();
    assert_eq!(
        result.dataset.rows(),
        &[vec![
            Value::Str("S".into()),
            Value::Str("P".into()),
            Value::Str("ALT".into()),
            Value::float(12.0),
            Value::float(10.0),
            Value::float(20.0)
        ]]
    );
    let different = b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,12,10\nS,P,ALT,12.0,10\n";
    let table = csv_source::parse_text_table(different, Default::default(), table_limits).unwrap();
    let attempt = plan.execute_observed(&table, limits());
    assert!(attempt.handler_counts.is_empty());
    let dataset::ExecutionError::MultipleValues {
        path,
        identifier,
        value_count,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("expected raw distinct readings")
    };
    assert_eq!(path, "columns.AVAL.derivation.source");
    assert_eq!(identifier, "LB.LBSTRESN");
    assert_eq!(value_count, 2);
    assert_eq!(
        identity.unwrap().values,
        vec![
            Value::Str("S".into()),
            Value::Str("P".into()),
            Value::Str("ALT".into())
        ]
    );
}

#[test]
fn captured_schema_and_occurrence_origins_survive_caller_buffers() {
    use yamaa_core::schema::SchemaSource;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let mut bytes =
        std::fs::read(root.join("benchmarks/schema-window-functions/spec.yaml")).unwrap();
    let captured = prepare(&schema, &bytes);
    drop(schema);
    bytes.fill(b'?');
    drop(bytes);
    assert!(captured.source().bytes.starts_with(b"schema_version:"));
    let d = captured.model().document();
    assert_eq!(captured.origins().len(), d.nodes().len());
    for origin in captured.origins() {
        match origin.source {
            SchemaSource::Input => assert!(origin.node < captured.raw().document.nodes().len()),
            SchemaSource::Default { module, .. } => {
                assert!(
                    origin.node
                        < captured.schema().structure().modules()[module]
                            .document
                            .nodes()
                            .len()
                );
                assert!(origin.node < captured.schema().locations()[module].len());
            }
        }
    }
    assert!(!captured.windows().is_empty());
    assert_eq!(
        captured.schema().sources().len(),
        captured.schema().locations().len()
    );
}

#[test]
fn inheritance_presence_and_capture_budgets_cannot_silently_fall_through() {
    use yamaa_adapters::specification_source::{Error, Limits};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    for raw in [
        b"parents: null".as_slice(),
        b"parents: []",
        b"parents: [parent.yaml]",
    ] {
        assert!(matches!(
            schema.prepare_standalone(Source {
                identity: "child.yaml".into(),
                bytes: raw.to_vec()
            }),
            Err(Error::InheritanceRequired)
        ));
    }
    let error = CapturedSchema::admit(
        vec![Source {
            identity: "schema.yaml".into(),
            bytes: b"bad yaml [".to_vec(),
        }],
        0,
        Limits {
            captured_bytes: 2,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, Error::Limit("captured_bytes")));
    let error = CapturedSchema::admit(
        vec![Source {
            identity: "schema.yaml".into(),
            bytes: vec![],
        }],
        0,
        Limits {
            identity_bytes: 2,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, Error::Limit("identity_bytes")));
}

#[test]
fn unsupported_operations_handlers_and_metadata_are_refused_before_binding() {
    use yamaa_engine::specification::{PrepareError, PreparedSpecification};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw =
        std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap();
    for (written, feature) in [
        (
            raw.replace("100 * (AVAL - BASE) / BASE", "LN(AVAL)"),
            "numeric_function",
        ),
        (
            raw.replace("input/lb.csv", "{path: input/lb.csv, ordinal: RECNO}"),
            "ordinal",
        ),
        (
            format!("{raw}\nmetadata:\n  label: reserved\n"),
            "reserved_metadata_key",
        ),
    ] {
        let captured = prepare(&schema, written.as_bytes());
        let error = PreparedSpecification::prepare(captured.model()).unwrap_err();
        let PrepareError::Unsupported(features) = error else {
            panic!("{error:?}")
        };
        assert!(
            features.iter().any(|f| f.operation == feature),
            "{features:?}"
        );
    }
    let written = raw.replace("100 * (AVAL - BASE) / BASE", "LB.LBSTRESN + 1");
    let captured = prepare(&schema, written.as_bytes());
    let prepared = PreparedSpecification::prepare(captured.model()).unwrap();
    let schema = TableSchema::new(vec![]).unwrap();
    assert!(matches!(
        prepared.bind(&schema),
        Err(yamaa_engine::specification::BindError::Invalid(_))
    ));
}

#[test]
fn binding_uses_shared_reference_and_dependency_findings() {
    use yamaa_core::reference_binding::Diagnostic;
    use yamaa_engine::specification::{BindError, BindFinding, PreparedSpecification};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw =
        std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap();
    let table = csv_source::parse_text_table(
        b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\n",
        Default::default(),
        TableLimits {
            max_rows: 0,
            max_columns: 5,
            max_batches: 1,
            max_cells: 0,
        },
    )
    .unwrap();
    for (expression, expected_suggestion) in [("MISSING + 1", false), ("LBSTRESN + 1", true)] {
        let written = raw.replace("100 * (AVAL - BASE) / BASE", expression);
        let captured = prepare(&schema, written.as_bytes());
        let prepared = PreparedSpecification::prepare(captured.model()).unwrap();
        let error = prepared.bind(table.schema()).unwrap_err();
        let BindError::Invalid(findings) = &error else {
            panic!("{error:?}")
        };
        let [BindFinding::OutputReference { finding, .. }] = findings.as_slice() else {
            panic!("{error:?}")
        };
        assert_eq!(
            *finding,
            if expected_suggestion {
                Diagnostic::UnresolvableName { dataset: 0 }
            } else {
                Diagnostic::UnknownField
            }
        );
    }
    let written = raw.replace("100 * (AVAL - BASE) / BASE", "PCHG + 1");
    let captured = prepare(&schema, written.as_bytes());
    assert!(matches!(
        PreparedSpecification::prepare(captured.model())
            .unwrap()
            .bind(table.schema()),
        Err(BindError::Invalid(findings)) if matches!(findings.as_slice(), [BindFinding::Dependencies { .. }])
    ));
}

#[test]
fn original_yaml_run_returns_complete_independent_portable_failures() {
    use serde_json::json;
    use yamaa_adapters::specification_run::PreparedRun;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    for (name, condition, requirement, column, expr, keys) in [
        (
            "negative-zero-division",
            "division_by_zero",
            "REQ-0430",
            "PCHG",
            "100 * (AVAL - BASE) / BASE",
            vec!["PILOT7", "P7-742", "ALT"],
        ),
        (
            "negative-integer-overflow",
            "integer_overflow",
            "REQ-0434",
            "CELLTOT",
            "CELLCNT * VOLUML",
            vec!["YAMAA-01", "YAMAA-01-102"],
        ),
    ] {
        let case = root.join("benchmarks").join(name);
        let captured = prepare(&schema, &std::fs::read(case.join("spec.yaml")).unwrap());
        let run = PreparedRun::prepare(captured).unwrap();
        assert_eq!(run.source().name, "LB");
        assert_eq!(run.source().path, "input/lb.csv");
        let mut context = json!({"expr":{"str":expr}});
        if condition == "integer_overflow" {
            context["minimum"] = json!({"int":"-9223372036854775808"});
            context["maximum"] = json!({"int":"9223372036854775807"});
            context["value"] = json!({"str":"9223372036854776832"});
        }
        let expected = json!({"protocol":"dataset/1","outcome":{
            "status":"condition","diagnostic":{"phase":"derivation","condition":condition,"requirement":requirement,
            "spec_paths":[format!("columns.{column}.derivation.compute")],"context":context,"source_span":{"start":"0","end":expr.len().to_string()},"operand_route":[]},
            "identity":{"position":"1","keys":keys.iter().map(|key|json!({"str":key})).collect::<Vec<_>>()}
        }});
        for _ in 0..2 {
            let response = run
                .execute_csv(&std::fs::read(case.join("input/lb.csv")).unwrap())
                .unwrap();
            assert!(response.table.is_none());
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&response.outcome).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn source_ingestion_failure_precedes_deferred_formula_diagnostics() {
    use yamaa_adapters::specification_run::{Error, PreparedRun};
    use yamaa_engine::specification::{BindError, BindFinding};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/negative-zero-division");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    for (expression, qualified) in [("AVAL +", false), ("LB.LBSTRESN + 1", true)] {
        let written = raw.replace("100 * (AVAL - BASE) / BASE", expression);
        let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
        assert!(matches!(
            run.execute_csv(b"A\n\"bad"),
            Err(Error::Source(_))
        ));
        let error = run
            .execute_csv(&std::fs::read(case.join("input/lb.csv")).unwrap())
            .err()
            .unwrap();
        let Error::Bind(BindError::Invalid(findings)) = &error else {
            panic!("{error:?}")
        };
        let path = match findings.as_slice() {
            [BindFinding::Numeric {
                path,
                expression: actual,
                ..
            }] if !qualified => {
                assert_eq!(actual, expression);
                path
            }
            [BindFinding::QualifiedNumericReference {
                path,
                expression: actual,
                ..
            }] if qualified => {
                assert_eq!(actual, expression);
                path
            }
            _ => panic!("wrong phase or condition: {error:?}"),
        };
        assert_eq!(path, "columns.PCHG.derivation.compute.expr");
    }
}

#[test]
fn compiler_diagnostics_match_independent_reference_paths_context_and_phase() {
    use serde_json::json;
    use yamaa_adapters::{specification_diagnostics::findings, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/negative-zero-division");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let data = std::fs::read(case.join("input/lb.csv")).unwrap();
    for (expr, condition, requirement, context) in [
        (
            "AVAL +",
            "invalid_numeric_expression",
            Some("REQ-0439"),
            json!({"expr":"AVAL +"}),
        ),
        (
            "LB.LBSTRESN + 1",
            "qualified_identifier",
            Some("REQ-0442"),
            json!({"expr":"LB.LBSTRESN + 1","identifier":"LB.LBSTRESN"}),
        ),
        (
            "BOGUS + 1",
            "unknown_field",
            None,
            json!({"identifier":"BOGUS"}),
        ),
        (
            "LBSTRESN + 1",
            "unresolvable_name",
            None,
            json!({"identifier":"LBSTRESN","suggestion":"LB.LBSTRESN"}),
        ),
    ] {
        let written = raw.replace("100 * (AVAL - BASE) / BASE", expr);
        let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
        let error = run.execute_csv(&data).err().unwrap();
        assert_eq!(
            findings(&error, Some(run.source())).unwrap(),
            vec![
                json!({"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":["columns.PCHG.derivation.compute.expr"],"context":context})
            ]
        );
    }
    let written = raw.replace("derivation: LB.LBSTRESN", "derivation: LB.BOGUS");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&data).err().unwrap();
    assert_eq!(
        findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.AVAL.derivation.source"],"context":{"identifier":"LB.BOGUS"}})
        ]
    );
    let written = raw.replace("100 * (AVAL - BASE) / BASE", "AVAL +");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(b"A\n\"bad").err().unwrap();
    assert_eq!(
        findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"ingest","condition":"source_quote_unterminated","requirement":"REQ-0851","spec_paths":["input.LB.path"],"context":{"dataset":"LB","path":"input/lb.csv","record":2,"field":1}})
        ]
    );
    let written = raw.replace(
        "columns: [STUDYID, USUBJID, PARAMCD, AVAL, BASE, PCHG]",
        "columns: [STUDYID, USUBJID, PARAMCD, AVAL, BASE, BOGUS]",
    );
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let response = run.execute_csv(&data).unwrap();
    let response: serde_json::Value = serde_json::from_str(&response.outcome).unwrap();
    assert_eq!(
        response["outcome"]["diagnostic"]["condition"],
        "division_by_zero"
    );
}

#[test]
fn preflight_reports_all_findings_in_reference_order_before_source_access() {
    use serde_json::json;
    use yamaa_adapters::{specification_diagnostics::findings, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw =
        std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap();
    let written = raw
        .replace("domain: ADLB", "domain: LB\nbase: BAD")
        .replace(
            "keys: [STUDYID, USUBJID, PARAMCD]",
            "keys: [STUDYID, USUBJID, PARAMCD, BOGUS]",
        )
        .replace(
            "    derivation:\n      compute:\n        expr: \"100 * (AVAL - BASE) / BASE\"",
            "",
        );
    let error = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap_err();
    assert_eq!(
        findings(&error, None).unwrap(),
        vec![
            json!({"phase":"validation","condition":"missing_derivation","spec_paths":["columns.PCHG.derivation"],"requirement":"REQ-0198","context":{"column":"PCHG"}}),
            json!({"phase":"validation","condition":"duplicate_identifier","spec_paths":["input.LB","domain"],"requirement":"REQ-0080","context":{"identifier":"LB"}}),
            json!({"phase":"validation","condition":"undeclared_column","spec_paths":["keys[3]"],"requirement":"REQ-0220","context":{"column":"BOGUS"}}),
            json!({"phase":"validation","condition":"driver_unavailable","spec_paths":["base"],"requirement":null,"context":{"dataset":"BAD"}}),
        ]
    );
}

#[test]
fn core_preflight_matches_independent_complete_findings() {
    use serde_json::Value;
    use yamaa_adapters::{specification_diagnostics, specification_run::PreparedRun};
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in include_str!("fixtures/preflight.tsv").lines().skip(1) {
        let fields = row.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 3);
        let error = PreparedRun::prepare(prepare(&schema, fields[1].as_bytes())).unwrap_err();
        let actual: Value =
            serde_json::from_str(&specification_diagnostics::failure(&error, None)).unwrap();
        let expected: Value = serde_json::from_str(fields[2]).unwrap();
        assert_eq!(actual, expected, "{}", fields[0]);
        count += 1;
    }
    assert_eq!(count, 5);
}

#[test]
fn core_output_declarations_match_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("fixtures/output_declarations.tsv"),
        "output",
        (5, 8),
        b"ID\n1\n",
    );
}

#[test]
fn core_grammar_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("fixtures/grammar_diagnostics.tsv"),
        "grammar",
        (7, 7),
        b"ID\n1\n",
    );
}

#[test]
fn core_binding_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("fixtures/binding_diagnostics.tsv"),
        "binding",
        (10, 11),
        b"ID,V\n1,2\n",
    );
}

#[test]
fn core_csv_profile_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("fixtures/csv_profile_diagnostics.tsv"),
        "csv",
        (13, 13),
        b"",
    );
}

#[test]
fn core_window_findings_match_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("fixtures/window_diagnostics.tsv"),
        "window",
        (3, 3),
        b"ID,V\n1,2\n",
    );
}

#[test]
fn core_predicate_matches_independent_complete_failed_report() {
    independent_failed_reports(
        include_str!("fixtures/predicate_diagnostics.tsv"),
        "predicate",
        (1, 1),
        b"ID,V\n1,2\n",
    );
}

fn independent_failed_reports(
    fixture: &str,
    prefix: &str,
    expected_counts: (usize, usize),
    content: &[u8],
) {
    use serde_json::Value;
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Port(usize, Arc<[u8]>);
    impl SourcePort for Port {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.0
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            assert!(maximum >= self.1.len());
            self.0 += 1;
            Ok(Arc::clone(&self.1))
        }
    }
    impl ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed output published")
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut cases = 0;
    let mut findings = 0;
    for row in fixture.lines().skip(1) {
        let fields = row.split('\t').collect::<Vec<_>>();
        assert!(matches!(fields.len(), 3 | 4));
        let run = PreparedRun::prepare(prepare(&schema, fields[1].as_bytes())).unwrap();
        let owned;
        let content = if fields.len() == 4 {
            owned = (0..fields[3].len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&fields[3][i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            owned.as_slice()
        } else {
            content
        };
        let mut port = Port(0, Arc::from(content));
        let attempt = run.execute_with_port(&mut port);
        let example = format!("{prefix}-{}", fields[0]);
        let result = specification_report::build_result(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: &example,
                specification: "spec.yaml",
                base_directory: ".",
            },
        )
        .unwrap();
        drop(run);
        drop(attempt);
        let expected: Value = serde_json::from_str(fields[2]).unwrap();
        assert_eq!(result.observations(), expected, "{}", fields[0]);
        assert!(result.output().is_none());
        for _ in 0..2 {
            assert!(matches!(
                result.save(&mut port),
                Err(yamaa_engine::specification_output::SaveError::FailedBuild)
            ));
            assert_eq!(result.observations(), expected);
        }
        assert_eq!(port.0, expected["source_reads"].as_array().unwrap().len());
        findings += expected["diagnostics"].as_array().unwrap().len();
        cases += 1;
    }
    assert_eq!((cases, findings), expected_counts);
}

#[test]
fn binding_collects_ordered_findings_before_dependency_diagnostics() {
    use serde_json::json;
    use yamaa_adapters::{specification_diagnostics::findings, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/negative-zero-division");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let data = std::fs::read(case.join("input/lb.csv")).unwrap();
    let unknown = json!({"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["columns.PCHG.derivation.compute.expr"],"context":{"identifier":"BOGUS"}});
    let cycle = json!({"phase":"validation","condition":"dependency_cycle","requirement":"REQ-0072","spec_paths":["columns.PCHG.derivation.compute"],"context":{"cycle":["PCHG","PCHG"]}});
    let written = raw
        .replace("derivation: LB.LBSTRESN", "derivation: LB.BOGUS")
        .replace("100 * (AVAL - BASE) / BASE", "BOGUS + PCHG");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&data).err().unwrap();
    assert_eq!(
        findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.AVAL.derivation.source"],"context":{"identifier":"LB.BOGUS"}}),
            unknown.clone(),
            cycle.clone()
        ]
    );
    let expr = "LB.LBSTRESN + BOGUS + LB.LBBLRESN + PCHG";
    let written = raw.replace("100 * (AVAL - BASE) / BASE", expr);
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&data).err().unwrap();
    assert_eq!(
        findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"qualified_identifier","requirement":"REQ-0442","spec_paths":["columns.PCHG.derivation.compute.expr"],"context":{"expr":expr,"identifier":"LB.LBSTRESN"}}),
            json!({"phase":"validation","condition":"qualified_identifier","requirement":"REQ-0442","spec_paths":["columns.PCHG.derivation.compute.expr"],"context":{"expr":expr,"identifier":"LB.LBBLRESN"}}),
            unknown,
            cycle
        ]
    );
}

#[test]
fn compiler_budgets_precede_owned_planning_and_preserve_policy_boundary() {
    use yamaa_adapters::{specification_diagnostics, specification_run};
    use yamaa_engine::specification::{CompilationLimits, PrepareError, PreparedSpecification};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw =
        std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap();
    // Two formulas each fit individually, but the sum exceeds the run's budget.
    let written = raw
        .replace(
            "derivation: LB.LBSTRESN",
            "derivation: {compute: {expr: '1 + 2'}}",
        )
        .replace("100 * (AVAL - BASE) / BASE", "3 + 4");
    let doc = prepare(&schema, written.as_bytes());
    for (resource, limits) in [
        (
            "columns",
            CompilationLimits {
                columns: 5,
                ..Default::default()
            },
        ),
        (
            "keys",
            CompilationLimits {
                keys: 2,
                ..Default::default()
            },
        ),
        (
            "inputs",
            CompilationLimits {
                inputs: 0,
                ..Default::default()
            },
        ),
        (
            "projected_columns",
            CompilationLimits {
                projected_columns: 5,
                ..Default::default()
            },
        ),
        (
            "model_text_bytes",
            CompilationLimits {
                model_text_bytes: 1,
                ..Default::default()
            },
        ),
        (
            "numeric_bytes",
            CompilationLimits {
                numeric_bytes: 9,
                ..Default::default()
            },
        ),
    ] {
        let error = PreparedSpecification::prepare_with_limits(doc.model(), limits).unwrap_err();
        assert!(
            matches!(error, PrepareError::Limit(actual) if actual == resource),
            "{error:?}"
        );
        let error = specification_run::Error::Prepare(error);
        assert!(specification_diagnostics::findings(&error, None).is_none());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&specification_diagnostics::failure(
                &error, None
            ))
            .unwrap(),
            serde_json::json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"prepare","code":"compiler_limit"}})
        );
    }
    PreparedSpecification::prepare_with_limits(
        doc.model(),
        CompilationLimits {
            numeric_bytes: 10,
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn actual_capture_observations_survive_errors_and_distinguish_cached_reads() {
    use yamaa_adapters::specification_run::{Error, PortError, PreparedRun, SourcePort};
    use yamaa_engine::specification::SourceDeclaration;
    struct Port {
        content: Arc<[u8]>,
        reads: usize,
        requests: usize,
        fail: bool,
    }
    impl SourcePort for Port {
        type Error = &'static str;
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            limit: usize,
        ) -> Result<Arc<[u8]>, Self::Error> {
            assert_eq!((&*source.name, &*source.path), ("LB", "input/lb.csv"));
            assert_eq!(limit, 8_388_608);
            self.requests += 1;
            if self.fail {
                return Err("resource_path_missing");
            }
            if self.requests == 1 {
                self.reads += 1;
            }
            Ok(Arc::clone(&self.content))
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/negative-zero-division");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
    let content: Arc<[u8]> = std::fs::read(case.join("input/lb.csv")).unwrap().into();
    let mut port = Port {
        content: Arc::clone(&content),
        reads: 0,
        requests: 0,
        fail: false,
    };
    for created in [1, 0] {
        let attempt = run.execute_with_port(&mut port);
        assert!(attempt.sources[0].read.captured);
        assert_eq!(attempt.sources[0].read.snapshots_created, Some(created));
        assert!(Arc::ptr_eq(
            attempt.sources[0].snapshot.as_ref().unwrap(),
            &content
        ));
        assert_eq!(attempt.sources[0].table.as_ref().unwrap().row_count(), 2);
        let response = attempt.result.unwrap();
        assert!(response.table.is_none());
        assert!(response.outcome.contains("division_by_zero"));
    }
    assert_eq!(port.requests, 2);
    let invalid = raw.replace("100 * (AVAL - BASE) / BASE", "AVAL +");
    let run = PreparedRun::prepare(prepare(&schema, invalid.as_bytes())).unwrap();
    let attempt = run.execute_with_port(&mut port);
    assert!(attempt.sources[0].table.is_some());
    assert!(matches!(
        attempt.result,
        Err(PortError::Run(Error::Bind(_)))
    ));
    port.content = Arc::from(b"A\n\"bad".as_slice());
    let attempt = run.execute_with_port(&mut port);
    assert!(attempt.sources[0].snapshot.is_some());
    assert!(attempt.sources[0].table.is_none());
    assert!(matches!(
        attempt.result,
        Err(PortError::Run(Error::Source(_)))
    ));
    port.fail = true;
    let attempt = run.execute_with_port(&mut port);
    assert!(!attempt.sources[0].read.captured);
    assert_eq!(attempt.sources[0].read.snapshots_created, Some(0));
    assert!(attempt.sources[0].snapshot.is_none() && attempt.sources[0].table.is_none());
    assert!(matches!(
        attempt.result,
        Err(PortError::Capture("resource_path_missing"))
    ));
}

#[test]
fn whole_failure_reports_match_reference_observations_from_actual_capture() {
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_engine::specification::SourceDeclaration;
    struct Files {
        base: std::path::PathBuf,
        reads: usize,
    }
    impl SourcePort for Files {
        type Error = std::io::Error;
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            limit: usize,
        ) -> Result<Arc<[u8]>, Self::Error> {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(self.base.join(&source.path))?
                .take((limit + 1) as u64)
                .read_to_end(&mut bytes)?;
            assert!(bytes.len() <= limit);
            self.reads += 1;
            Ok(bytes.into())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    for name in [
        "negative-zero-division",
        "negative-integer-overflow",
        "negative-formula-flag",
        "negative-row-aggregate",
        "negative-row-no-prior",
    ] {
        let case = root.join("benchmarks").join(name);
        let run = PreparedRun::prepare(prepare(
            &schema,
            &std::fs::read(case.join("spec.yaml")).unwrap(),
        ))
        .unwrap();
        let mut port = Files {
            base: case,
            reads: 0,
        };
        let attempt = run.execute_with_port(&mut port);
        assert_eq!(port.reads, 1);
        // Independently authored complete failure truth; benchmark source and
        // diagnostic declarations also receive a separate reference comparison.
        let mut expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/specs")
                    .join(format!("{name}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let actual = specification_report::failure(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: expected["runtime_version"].as_str().unwrap(),
                engine_version: expected["engine_version"].as_str().unwrap(),
                example: name,
                specification: "spec.yaml",
                base_directory: ".",
            },
        )
        .unwrap();
        expected["backend"] = serde_json::json!("rust");
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn declared_source_types_preserve_ingestion_diagnostics_before_formula_errors() {
    use serde_json::json;
    use yamaa_adapters::{specification_diagnostics::findings, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/negative-zero-division");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let written = raw
        .replace(
            "LB: input/lb.csv",
            "LB: {path: input/lb.csv, types: {LBSTRESN: int}}",
        )
        .replace("100 * (AVAL - BASE) / BASE", "AVAL +");
    assert_ne!(written, raw);
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    assert_eq!(run.source().types.len(), 1);
    let error = run
        .execute_csv(b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,42.5,0")
        .err()
        .unwrap();
    assert_eq!(
        findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"ingest","condition":"field_parse_failed","requirement":"REQ-0536","spec_paths":["input.LB.types.LBSTRESN"],"context":{"dataset":"LB","field":"LBSTRESN","type":"int","value":"42.5"}})
        ]
    );
    let written = written.replace("{LBSTRESN: int}", "{LBSTRESN: int, ZZ: float, AA: int}");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run
        .execute_csv(b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,42.5,0")
        .err()
        .unwrap();
    assert_eq!(
        findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0532","spec_paths":["input.LB.types.ZZ"],"context":{"dataset":"LB","field":"ZZ"}})
        ]
    );
}

#[test]
fn original_ordered_sum_yaml_executes_all_seventeen_rows_with_exact_float_bits() {
    use yamaa_adapters::typed_csv;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/adam-adlb-ordered-sum");
    let document = prepare(&schema, &std::fs::read(case.join("spec.yaml")).unwrap());
    let compiled =
        yamaa_engine::specification::PreparedSpecification::prepare(document.model()).unwrap();
    let table_limits = TableLimits {
        max_rows: 100,
        max_columns: 64,
        max_batches: 1,
        max_cells: 6400,
    };
    let source = typed_csv::parse(
        &std::fs::read(case.join("input/lb.csv")).unwrap(),
        &compiled.source().types,
        Default::default(),
        table_limits,
    )
    .unwrap();
    let plan = compiled.bind(source.schema()).unwrap();
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(result.dataset.rows().len(), 17);
    let projection = compiled
        .projection()
        .iter()
        .map(|name| {
            result
                .dataset
                .schema()
                .columns()
                .iter()
                .position(|c| &c.name == name)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let bytes = yamaa_adapters::csv_artifact::render(&result.dataset, &projection, 4096).unwrap();
    assert_eq!(
        bytes,
        std::fs::read(case.join("expected/adlb.csv")).unwrap()
    );
    let expected = typed_csv::parse(
        &std::fs::read(case.join("expected/adlb.csv")).unwrap(),
        &[("AVAL".into(), yamaa_core::value::ColumnType::Float)],
        Default::default(),
        table_limits,
    )
    .unwrap();
    for (column, name) in compiled.projection().iter().enumerate() {
        assert_eq!(&expected.schema().columns()[column].name, name);
        let actual_column = result
            .dataset
            .schema()
            .columns()
            .iter()
            .position(|c| &c.name == name)
            .unwrap();
        assert_eq!(
            result.dataset.schema().columns()[actual_column].kind,
            expected.schema().columns()[column].kind
        );
        for row in 0..17 {
            assert_eq!(
                result.dataset.cell(row, actual_column).unwrap(),
                expected.cell(row, column).unwrap(),
                "row {row} column {name}"
            );
        }
    }
    let values = result
        .dataset
        .schema()
        .columns()
        .iter()
        .position(|c| c.name == "AVAL")
        .unwrap();
    let Value::Float(first) = result.dataset.rows()[12][values] else {
        panic!()
    };
    let Value::Float(second) = result.dataset.rows()[13][values] else {
        panic!()
    };
    assert_eq!(first.get().to_bits(), 0.6000000000000001f64.to_bits());
    assert_eq!(second.get().to_bits(), 0.6f64.to_bits());
    assert_eq!(result.dataset.rows()[16][values], Value::Missing);
    assert_eq!(result.verifications.len(), 2);
    assert!(result.verifications.iter().all(|v| v.failed_count == 0));
}

#[test]
fn aggregate_grammar_findings_follow_typed_ingestion_and_retain_original_path() {
    use serde_json::json;
    use yamaa_adapters::{specification_diagnostics::findings, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/adam-adlb-ordered-sum");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let data = std::fs::read(case.join("input/lb.csv")).unwrap();
    for (expr, condition, requirement, context) in [
        (
            "SUM(AVAL)",
            "invalid_aggregate_context",
            "REQ-0329",
            json!({"expr":"SUM(AVAL)","reason":"a grouped row aggregate reads its row driver"}),
        ),
        (
            "SUM(ABSENT)",
            "invalid_aggregate_context",
            "REQ-0329",
            json!({"expr":"SUM(ABSENT)","reason":"a grouped row aggregate reads its row driver"}),
        ),
        (
            "SUM(OTHER.X)",
            "invalid_aggregate_context",
            "REQ-0329",
            json!({"expr":"SUM(OTHER.X)","reason":"a grouped row aggregate reads 'LB', not 'OTHER'"}),
        ),
        (
            "SUM(LB.ABSENT)",
            "unknown_field",
            "REQ-0103",
            json!({"identifier":"LB.ABSENT"}),
        ),
        (
            "SUM(",
            "invalid_aggregate_expression",
            "REQ-0499",
            json!({"expr":"SUM("}),
        ),
        (
            "SUM(SUM(LB.LBSTRESN))",
            "nested_reduction",
            "REQ-0502",
            json!({"expr":"SUM(SUM(LB.LBSTRESN))","outer":"SUM","inner":"SUM"}),
        ),
        (
            "SUM(LB.LBSTRESN, LB.LBSTRESN)",
            "invalid_aggregate_expression",
            "REQ-0499",
            json!({"expr":"SUM(LB.LBSTRESN, LB.LBSTRESN)"}),
        ),
    ] {
        let run = PreparedRun::prepare(prepare(
            &schema,
            raw.replace("SUM(LB.LBSTRESN)", expr).as_bytes(),
        ))
        .unwrap();
        let error = run.execute_csv(&data).err().unwrap();
        assert_eq!(
            findings(&error, Some(run.source())).unwrap(),
            vec![
                json!({"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":["rows[1].derivations.AVAL.aggregate"],"context":context})
            ]
        );
        let invalid_data = String::from_utf8(data.clone())
            .unwrap()
            .replacen(",0.1", ",invalid", 1);
        let error = run.execute_csv(invalid_data.as_bytes()).err().unwrap();
        assert_eq!(
            findings(&error, Some(run.source())).unwrap(),
            vec![
                json!({"phase":"ingest","condition":"field_parse_failed","requirement":"REQ-0536","spec_paths":["input.LB.types.LBSTRESN"],"context":{"dataset":"LB","field":"LBSTRESN","type":"float","value":"invalid"}})
            ]
        );
    }
}

#[test]
fn declared_field_and_total_row_expression_budgets_precede_compiler_ownership() {
    use yamaa_engine::specification::{CompilationLimits, PrepareError, PreparedSpecification};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw = std::fs::read(root.join("benchmarks/adam-adlb-ordered-sum/spec.yaml")).unwrap();
    let document = prepare(&schema, &raw);
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            document.model(),
            CompilationLimits {
                source_fields: 0,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("source_fields"))
    ));
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            document.model(),
            CompilationLimits {
                numeric_bytes: 3,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("numeric_bytes"))
    ));
}

#[test]
fn original_success_report_follows_actual_atomic_publication_and_output_failures_do_not_publish() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, CompleteError, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_engine::specification::SourceDeclaration;
    struct Captured {
        bytes: Arc<[u8]>,
        reads: usize,
    }
    impl SourcePort for Captured {
        type Error = std::convert::Infallible;
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            limit: usize,
        ) -> Result<Arc<[u8]>, Self::Error> {
            assert_eq!(source.path, "input/lb.csv");
            assert!(self.bytes.len() <= limit);
            self.reads = 1;
            Ok(self.bytes.clone())
        }
    }
    struct Publisher {
        directory: std::path::PathBuf,
        calls: usize,
        fail: bool,
    }
    impl ArtifactPort for Publisher {
        type Error = &'static str;
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error> {
            self.calls += 1;
            if self.fail {
                return Err("original publication error");
            }
            assert_eq!(path, "adlb.csv");
            use std::io::Write;
            let temporary = self.directory.join("candidate.csv");
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .unwrap();
            file.write_all(bytes).unwrap();
            file.sync_all().unwrap();
            drop(file);
            std::fs::rename(temporary, self.directory.join(path)).unwrap();
            Ok(())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/adam-adlb-ordered-sum");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "yamaa-original-success-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut publisher = Publisher {
        directory: directory.clone(),
        calls: 0,
        fail: false,
    };
    let mut port = Captured {
        bytes: std::fs::read(case.join("input/lb.csv")).unwrap().into(),
        reads: 0,
    };
    let id = || Identity {
        runtime: "python",
        runtime_version: "fixture-runtime",
        engine_version: "fixture-engine",
        example: "adam-adlb-ordered-sum",
        specification: "spec.yaml",
        base_directory: ".",
    };
    let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
    let expected: Json =
        serde_json::from_str(include_str!("fixtures/specs/adam-adlb-ordered-sum.json")).unwrap();
    let attempt = run.execute_with_port(&mut port);
    let report = specification_report::complete(&run, &attempt, id(), &mut publisher).unwrap();
    assert_eq!(report, expected);
    assert_eq!(publisher.calls, 1);
    assert_eq!(
        std::fs::read(directory.join("adlb.csv")).unwrap(),
        std::fs::read(case.join("expected/adlb.csv")).unwrap()
    );
    publisher.fail = true;
    assert!(matches!(
        specification_report::complete(&run, &attempt, id(), &mut publisher),
        Err(CompleteError::Publish("original publication error"))
    ));
    assert_eq!(publisher.calls, 2);
    publisher.fail = false;
    let run = PreparedRun::prepare(prepare(
        &schema,
        raw.replace("path: adlb.csv", "path: adlb.bad").as_bytes(),
    ))
    .unwrap();
    let attempt = run.execute_with_port(&mut port);
    let report = specification_report::complete(&run, &attempt, id(), &mut publisher).unwrap();
    let mut expected = expected;
    expected["outcome"] = json!("failure");
    expected["artifacts"] = json!([]);
    expected["tables"].as_array_mut().unwrap().pop();
    expected["nodes"][0]["outcome"] = json!("failure");
    expected["diagnostics"] = json!([{"phase":"validation","condition":"unknown_artifact_profile","requirement":"REQ-0760","spec_paths":["output.path"],"context":{"path":"adlb.bad","permitted":[".csv",".parquet"]}}]);
    expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
    expected["source_reads"][0]["snapshots_created"] = json!(0);
    assert_eq!(report, expected);
    assert_eq!(publisher.calls, 2);

    let run = PreparedRun::prepare(prepare(
        &schema,
        raw.replace("min: 17\n      max: 17", "min: 18\n      max: 18")
            .as_bytes(),
    ))
    .unwrap();
    let attempt = run.execute_with_port(&mut port);
    let actual = specification_report::complete(&run, &attempt, id(), &mut publisher).unwrap();
    let diagnostic = json!({"phase":"verification","condition":"row_count_failed","requirement":"REQ-0385","spec_paths":["verifications[1].row_count"],"context":{"count":17,"failure_count":1,"keys":[{}]}});
    expected["diagnostics"] = json!([diagnostic.clone()]);
    expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
    let mut failure = diagnostic;
    failure["severity"] = json!("error");
    failure["offending_keys"] = json!([{}]);
    failure["log_context"] = failure["context"].clone();
    expected["verifications"][1]["failure"] = failure;
    assert_eq!(actual, expected);
    assert_eq!(publisher.calls, 2);
    // All checks execute despite an earlier data failure; output diagnostics
    // remain later than verification and no derived/accepted artifact is exposed.
    let written = raw
        .replace(
            "columns: [STUDYID, USUBJID, PARAMCD, AVISIT]",
            "columns: [USUBJID]",
        )
        .replace("path: adlb.csv", "path: adlb.bad");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let actual = specification_report::complete(
        &run,
        &run.execute_with_port(&mut port),
        id(),
        &mut publisher,
    )
    .unwrap();
    let original: Json =
        serde_json::from_str(include_str!("fixtures/specs/adam-adlb-ordered-sum.json")).unwrap();
    let rows = original["tables"][1]["rows"].as_array().unwrap();
    let keys=[0,1,2,12,3,4,5,13,6,14,7,8,9,15,10,11,16].into_iter().map(|row|json!({"STUDYID":rows[row][0]["value"],"USUBJID":rows[row][1]["value"],"PARAMCD":rows[row][3]["value"],"AVISIT":rows[row][2]["value"]})).collect::<Vec<_>>();
    let diagnostic = json!({"phase":"verification","condition":"unique_failed","requirement":"REQ-0381","spec_paths":["verifications[0].unique"],"context":{"columns":["USUBJID"],"failure_count":5,"keys":&keys[..5]}});
    expected["diagnostics"] = json!([diagnostic.clone()]);
    expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
    let mut failure = diagnostic;
    failure["severity"] = json!("error");
    failure["offending_keys"] = json!(keys);
    failure["log_context"] = failure["context"].clone();
    failure["log_context"]["keys"] = failure["offending_keys"].clone();
    expected["verifications"][0]["evaluated_count"] = json!(5);
    expected["verifications"][0]["failure"] = failure;
    expected["verifications"][1]["failure"] = Json::Null;
    assert_eq!(actual, expected);
    assert_eq!(publisher.calls, 2);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn verification_declarations_preserve_completed_prefix_and_phase_precedence() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_engine::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            self.reads += 1;
            Ok(self.bytes.clone())
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("invalid verification must never publish")
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/adam-adlb-ordered-sum");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let prefix = raw.split("verifications:").next().unwrap();
    let bytes = std::fs::read(case.join("input/lb.csv")).unwrap();
    let id = || Identity {
        runtime: "python",
        runtime_version: "fixture-runtime",
        engine_version: "fixture-engine",
        example: "adam-adlb-ordered-sum",
        specification: "spec.yaml",
        base_directory: ".",
    };
    // Each valid first check records its identity and failure before the later
    // declaration aborts. Earlier data failures are kept in the ledger, while
    // the later declaration is the execution diagnostic (REQ-1177).
    for (declaration, condition, requirement, suffix, reason) in [
        (
            "unique: {columns: []}",
            "invalid_declaration",
            "REQ-0397",
            "unique",
            "columns names at least one column",
        ),
        (
            "unique: {columns: [ABSENT]}",
            "unknown_field",
            "REQ-0405",
            "unique.columns",
            "unknown column 'ABSENT'",
        ),
        (
            "row_count: {}",
            "invalid_declaration",
            "REQ-0399",
            "row_count",
            "row_count requires one bound",
        ),
        (
            "row_count: {min: 19, max: 18}",
            "invalid_declaration",
            "REQ-0399",
            "row_count",
            "row_count min exceeds max",
        ),
        (
            "row_count: {id: \"a'b\", min: 0}",
            "duplicate_identifier",
            "REQ-0398",
            "row_count",
            "verification id \"a'b\" repeats verifications[0].row_count",
        ),
    ] {
        for minimum in [0, 18] {
            let written = format!("{prefix}verifications:\n  - row_count: {{id: \"a'b\", min: {minimum}}}\n  - {declaration}\n  - row_count: {{min: 0}}\n");
            let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
            let mut host = Host {
                bytes: bytes.clone().into(),
                reads: 0,
            };
            let attempt = run.execute_with_port(&mut host);
            let actual = specification_report::complete(&run, &attempt, id(), &mut host).unwrap();
            assert_eq!(host.reads, 1);
            assert_eq!(actual["outcome"], "failure");
            assert_eq!(actual["artifacts"], json!([]));
            assert_eq!(actual["tables"].as_array().unwrap().len(), 1);
            assert_eq!(actual["verifications"].as_array().unwrap().len(), 1);
            let record = &actual["verifications"][0];
            assert_eq!(record["verification_id"], "a'b");
            assert_eq!(record["evaluated_count"], 1);
            if minimum == 0 {
                assert_eq!(record["failure"], Json::Null);
            } else {
                assert_eq!(record["failure"]["condition"], "row_count_failed");
                assert_eq!(record["failure"]["context"]["verification_id"], "a'b");
                assert_eq!(record["failure"]["log_context"]["verification_id"], "a'b");
            }
            assert_eq!(
                actual["diagnostics"],
                json!([{"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":[format!("verifications[1].{suffix}")],"context":{"reason":reason}}])
            );
        }
    }
    let written = format!("{prefix}verifications:\n  - row_count: {{}}\n");
    // Derivation grammar is deferred until ingestion but is still before checks.
    let invalid_formula = written.replace("SUM(LB.LBSTRESN)", "SUM(");
    let run = PreparedRun::prepare(prepare(&schema, invalid_formula.as_bytes())).unwrap();
    let mut host = Host {
        bytes: bytes.clone().into(),
        reads: 0,
    };
    let actual =
        specification_report::complete(&run, &run.execute_with_port(&mut host), id(), &mut host)
            .unwrap();
    assert_eq!(actual["verifications"], json!([]));
    assert_eq!(
        actual["diagnostics"][0]["spec_paths"],
        json!(["rows[1].derivations.AVAL.aggregate"])
    );
    // A missing output key is reached after derivation and before declaration checks.
    let run = PreparedRun::prepare(prepare(
        &schema,
        written
            .replace("derivation: LB.STUDYID", "derivation: {literal: null}")
            .as_bytes(),
    ))
    .unwrap();
    let actual =
        specification_report::complete(&run, &run.execute_with_port(&mut host), id(), &mut host)
            .unwrap();
    assert_eq!(actual["verifications"], json!([]));
    assert_eq!(actual["diagnostics"][0]["condition"], "missing_key");
    // Even a later unsupported operation prevents all source requests; it is
    // not hidden behind an earlier language declaration finding.
    let unsupported = format!("{written}  - assert: {{require: 'TRUE'}}\n");
    assert!(matches!(
        PreparedRun::prepare(prepare(&schema, unsupported.as_bytes())),
        Err(yamaa_adapters::specification_run::Error::Prepare(
            yamaa_engine::specification::PrepareError::Unsupported(_)
        ))
    ));
}

#[test]
fn row_group_declarations_and_shared_scope_keep_authored_diagnostics() {
    use serde_json::json;
    use yamaa_adapters::{specification_diagnostics, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw =
        std::fs::read_to_string(root.join("benchmarks/adam-adlb-ordered-sum/spec.yaml")).unwrap();
    let bytes = std::fs::read(root.join("benchmarks/adam-adlb-ordered-sum/input/lb.csv")).unwrap();
    for (replacement, condition, requirement, context) in [
        (
            "[]",
            "invalid_field_type",
            "REQ-0065",
            json!({"row":"total","group_by":[]}),
        ),
        (
            "[LB.STUDYID, LB.STUDYID]",
            "invalid_field_type",
            "REQ-0065",
            json!({"row":"total","group_by":["LB.STUDYID","LB.STUDYID"]}),
        ),
        (
            "[STUDYID]",
            "unknown_field",
            "REQ-0066",
            json!({"row":"total","identifier":"STUDYID","dataset":"LB"}),
        ),
    ] {
        let written = raw.replace(
            "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT]",
            &format!("group_by: {replacement}"),
        );
        let error = PreparedRun::prepare(prepare(&schema, written.as_bytes()))
            .expect_err("preflight without source");
        assert_eq!(specification_diagnostics::findings(&error,None).unwrap(),json!([{"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":["rows[1].group_by"],"context":context}]).as_array().unwrap().clone());
    }
    let written = raw
        .replace("id: total", "id: total\n    dataset: ABSENT")
        .replace("    group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT]\n", "");
    let error = PreparedRun::prepare(prepare(&schema, written.as_bytes()))
        .err()
        .unwrap();
    assert_eq!(specification_diagnostics::findings(&error,None).unwrap(),json!([{"phase":"validation","condition":"driver_unavailable","requirement":null,"spec_paths":["rows[1].dataset"],"context":{"row":"total","dataset":"ABSENT"}}]).as_array().unwrap().clone());
    let written = raw.replace(
        "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT]",
        "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT, LB.ABSENT]",
    );
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&bytes).err().unwrap();
    assert_eq!(
        specification_diagnostics::findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["rows[1].group_by[3]"],"context":{"identifier":"LB.ABSENT"}})
        ]
    );
    for (written, path, requirement, context) in [
        (
            raw.replace("PARAM: {literal: Total of Components}", "PARAM: LB.LBTEST"),
            "rows[1].derivations.PARAM.source",
            "REQ-0067",
            json!({"identifier":"LB.LBTEST","row":"total","dataset":"LB"}),
        ),
        (
            raw.replace("derivation: LB.VISIT", "derivation: LB.LBTEST"),
            "columns.AVISIT.derivation.source",
            "REQ-0107",
            json!({"identifier":"LB.LBTEST","dataset":"LB"}),
        ),
    ] {
        let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
        let error = run
            .execute_csv(&bytes)
            .err()
            .expect("scope finding after ingestion");
        assert_eq!(specification_diagnostics::findings(&error,Some(run.source())).unwrap(),json!([{"phase":"validation","condition":"ungrouped_driver_field","requirement":requirement,"spec_paths":[path],"context":context}]).as_array().unwrap().clone());
    }
}

#[test]
fn original_sum_reports_nonnumeric_values_but_publishes_all_missing_groups() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_engine::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
        published: Vec<Vec<u8>>,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            self.reads += 1;
            Ok(self.bytes.clone())
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            assert_eq!(path, "adlb.csv");
            self.published.push(bytes.to_vec());
            Ok(())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/adam-adlb-ordered-sum");
    let raw = std::fs::read_to_string(case.join("spec.yaml"))
        .unwrap()
        .replace("LBSTRESN: float", "LBSTRESN: str");
    let input = std::fs::read_to_string(case.join("input/lb.csv")).unwrap();
    let id = || Identity {
        runtime: "python",
        runtime_version: "fixture-runtime",
        engine_version: "fixture-engine",
        example: "adam-adlb-ordered-sum",
        specification: "spec.yaml",
        base_directory: ".",
    };
    for all_missing in [false, true] {
        let mut expected: Json =
            serde_json::from_str(include_str!("fixtures/specs/adam-adlb-ordered-sum.json"))
                .unwrap();
        expected["tables"][0]["types"][6] = json!("str");
        let mut source = String::new();
        for (i, line) in input.lines().enumerate() {
            if i == 0 {
                source.push_str(line);
                source.push('\n');
                continue;
            }
            let (fields, value) = line.rsplit_once(',').unwrap();
            let value = if all_missing { "" } else { value };
            source.push_str(&format!("{fields},{value}\n"));
            expected["tables"][0]["rows"][i - 1][6] = if value.is_empty() {
                json!({"type":"missing","value":null})
            } else {
                json!({"type":"str","value":value})
            };
        }
        let mut host = Host {
            bytes: source.into_bytes().into(),
            reads: 0,
            published: Vec::new(),
        };
        let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
        let attempt = run.execute_with_port(&mut host);
        let actual = specification_report::complete(&run, &attempt, id(), &mut host).unwrap();
        if all_missing {
            for row in expected["tables"][1]["rows"].as_array_mut().unwrap() {
                row[5] = json!({"type":"missing","value":null});
            }
            let original = std::fs::read_to_string(case.join("expected/adlb.csv")).unwrap();
            let records = original
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        line.to_owned()
                    } else {
                        let mut cells = line.split(',').collect::<Vec<_>>();
                        cells[5] = "";
                        cells.join(",")
                    }
                })
                .collect::<Vec<_>>();
            let content = format!("{}\n", records.join("\n"));
            expected["artifacts"][0]["records"] = json!(records);
            expected["artifacts"][0]["content"] = json!(content);
            expected["artifacts"][0]["byte_length"] = json!(content.len());
            assert_eq!(host.published, vec![content.into_bytes()]);
        } else {
            expected["outcome"] = json!("failure");
            expected["artifacts"] = json!([]);
            expected["verifications"] = json!([]);
            expected["tables"].as_array_mut().unwrap().pop();
            expected["diagnostics"] = json!([{"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0510","spec_paths":["rows[1].derivations.AVAL.aggregate"],"context":{"expr":"SUM(LB.LBSTRESN)","reducer":"SUM","source":"LB.LBSTRESN","expected":"numeric","actual":"str"}}]);
            expected["nodes"][0]["outcome"] = json!("failure");
            expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
            assert!(host.published.is_empty());
        }
        assert_eq!(host.reads, 1);
        assert_eq!(actual, expected);
    }
}

#[test]
fn row_defaults_are_inherited_per_template_and_coverage_errors_precede_io() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_diagnostics,
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_engine::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
        published: Vec<Vec<u8>>,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            self.reads += 1;
            Ok(self.bytes.clone())
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            assert_eq!(path, "adlb.csv");
            self.published.push(bytes.to_vec());
            Ok(())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/adam-adlb-ordered-sum");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let written = raw
        .replace(
            "label: Parameter\n",
            "label: Parameter\n    derivation: LB.LBTEST\n",
        )
        .replace("      PARAM: LB.LBTEST\n", "")
        .replace(
            "label: Derivation Type\n",
            "label: Derivation Type\n    derivation: {literal: null}\n",
        )
        .replace("      DTYPE: {literal: null}\n", "");
    let mut host = Host {
        bytes: std::fs::read(case.join("input/lb.csv")).unwrap().into(),
        reads: 0,
        published: Vec::new(),
    };
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let attempt = run.execute_with_port(&mut host);
    let actual = specification_report::complete(
        &run,
        &attempt,
        Identity {
            runtime: "python",
            runtime_version: "fixture-runtime",
            engine_version: "fixture-engine",
            example: "adam-adlb-ordered-sum",
            specification: "spec.yaml",
            base_directory: ".",
        },
        &mut host,
    )
    .unwrap();
    let expected: Json =
        serde_json::from_str(include_str!("fixtures/specs/adam-adlb-ordered-sum.json")).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(host.reads, 1);
    assert_eq!(
        host.published,
        vec![std::fs::read(case.join("expected/adlb.csv")).unwrap()]
    );
    // The inherited declaration retains its column path, although it executes
    // at row construction for the template without an override.
    let bad = written.replace("derivation: LB.LBTEST", "derivation: LB.ABSENT");
    let run = PreparedRun::prepare(prepare(&schema, bad.as_bytes())).unwrap();
    let error = run.execute_csv(&host.bytes).err().unwrap();
    assert_eq!(
        specification_diagnostics::findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.PARAM.derivation.source"],"context":{"identifier":"LB.ABSENT"}})
        ]
    );
    let bad = raw.replace(
        "label: Analysis Value\n",
        "label: Analysis Value\n    derivation: {aggregate: {expr: 'SUM(LB.LBSTRESN)'}}\n",
    );
    let error = PreparedRun::prepare(prepare(&schema, bad.as_bytes()))
        .expect_err("dataset aggregate is not a row default");
    assert_eq!(
        specification_diagnostics::findings(&error, None).unwrap(),
        vec![
            json!({"phase":"validation","condition":"duplicate_derivation","requirement":"REQ-1260","spec_paths":["columns.AVAL.derivation"],"context":{"column":"AVAL","rows":["collected","total"]}})
        ]
    );
    // Unknown row names are reported first, then missing coverage in declared
    // column order, then the independently conflicting root filter.
    let bad = raw
        .replace("domain: ADLB", "domain: ADLB\nfilter: 'TRUE'")
        .replace("      PARAM: LB.LBTEST\n", "")
        .replacen(
            "    derivations:\n",
            "    derivations:\n      ABSENT: {literal: ignored}\n",
            1,
        );
    let error = PreparedRun::prepare(prepare(&schema, bad.as_bytes()))
        .expect_err("preflight without source");
    assert_eq!(
        specification_diagnostics::findings(&error, None).unwrap(),
        vec![
            json!({"phase":"validation","condition":"undeclared_column","requirement":null,"spec_paths":["rows[0].derivations.ABSENT"],"context":{"column":"ABSENT"}}),
            json!({"phase":"validation","condition":"missing_derivation","requirement":"REQ-0200","spec_paths":["columns.PARAM.derivation"],"context":{"column":"PARAM","rows":["collected"]}}),
            json!({"phase":"validation","condition":"conflicting_row_construction","requirement":"REQ-1171","spec_paths":["filter","rows"],"context":{}}),
        ]
    );
}

#[test]
fn original_lookup_compiles_multiple_schemas_and_executes_named_selections() {
    use yamaa_core::specification::{BindError, PreparedSpecification};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/schema-lookup");
    let original = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    // Driver position never changes authored capture order or secondary indices.
    for raw in [
        original.clone(),
        original.replace(
            "  DM: input/dm.csv\n  AE: input/ae.csv",
            "  AE: input/ae.csv\n  DM: input/dm.csv",
        ),
    ] {
        let document = prepare(&schema, raw.as_bytes());
        let prepared = PreparedSpecification::prepare(document.model()).unwrap();
        assert_eq!(prepared.sources().len(), 3);
        assert_eq!(prepared.source().name, "DM");
        let tables = prepared
            .sources()
            .iter()
            .map(|source| {
                csv_source::parse_text_table(
                    &std::fs::read(case.join(&source.path)).unwrap(),
                    Default::default(),
                    TableLimits {
                        max_rows: 100,
                        max_columns: 64,
                        max_cells: 6400,
                        max_batches: 1,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            prepared.bind(tables[prepared.driver_index()].schema()),
            Err(BindError::SourceCount)
        ));
        let schemas = tables.iter().map(TableAccess::schema).collect::<Vec<_>>();
        let plan = prepared.bind_sources(&schemas).unwrap();
        let secondary = tables
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != prepared.driver_index())
            .map(|(_, table)| table as &dyn TableAccess<Error = _>)
            .collect::<Vec<_>>();
        for _ in 0..2 {
            let attempt = plan.execute_observed_sources(
                &tables[prepared.driver_index()],
                &secondary,
                limits(),
            );
            let execution = attempt.result.unwrap();
            let bytes =
                yamaa_adapters::csv_artifact::render(&execution.dataset, &[0, 1, 2, 3], 8192)
                    .unwrap();
            assert_eq!(
                bytes,
                std::fs::read(case.join("expected/adsl.csv")).unwrap()
            );
            assert_eq!(bytes.len(), 138);
            assert_eq!(execution.verifications.len(), 1);
            assert_eq!(execution.verifications[0].failed_count, 0);
            let actual = attempt
                .handler_counts
                .iter()
                .map(|entry| (entry.spec_path.as_str(), entry.handler.name(), entry.count))
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                [
                    (
                        "columns.DTHDY.derivation.source.multiple_matches",
                        "multiple_matches",
                        1
                    ),
                    ("columns.DTHDY.derivation.source.no_match", "no_match", 2),
                    (
                        "columns.DTHCAUS.derivation.source.multiple_matches",
                        "multiple_matches",
                        1
                    ),
                    ("columns.DTHCAUS.derivation.source.no_match", "no_match", 2),
                    ("columns.DTHPTERM.derivation.source.no_match", "no_match", 2),
                ]
            );
        }
    }
}

#[test]
fn original_lookup_complete_report_uses_all_captured_sources_and_actual_handlers() {
    use serde_json::{json, Value as Json};
    use std::collections::BTreeMap;
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        content: BTreeMap<String, Arc<[u8]>>,
        seen: Vec<String>,
        reads: usize,
        requests: Vec<String>,
        published: Vec<Vec<u8>>,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            self.requests.push(source.name.clone());
            if !self.seen.contains(&source.name) {
                self.seen.push(source.name.clone());
                self.reads += 1;
            }
            let bytes = Arc::clone(&self.content[&source.path]);
            assert!(bytes.len() <= maximum);
            Ok(bytes)
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            assert_eq!(path, "adsl.csv");
            self.published.push(bytes.to_vec());
            Ok(())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-lookup");
    let schema = schema(&root.join("yaml"));
    let prepared = PreparedRun::prepare(prepare(
        &schema,
        &std::fs::read(case.join("spec.yaml")).unwrap(),
    ))
    .unwrap();
    let content = prepared
        .compiled()
        .sources()
        .iter()
        .map(|source| {
            (
                source.path.clone(),
                Arc::from(std::fs::read(case.join(&source.path)).unwrap()),
            )
        })
        .collect();
    let mut host = Host {
        content,
        seen: Vec::new(),
        reads: 0,
        requests: Vec::new(),
        published: Vec::new(),
    };
    for created in [1, 0] {
        let attempt = prepared.execute_with_port(&mut host);
        assert_eq!(attempt.sources.len(), 3);
        let id = Identity {
            runtime: "python",
            runtime_version: "fixture-runtime",
            engine_version: "fixture-engine",
            example: "schema-lookup",
            specification: "spec.yaml",
            base_directory: ".",
        };
        let actual = specification_report::complete(&prepared, &attempt, id, &mut host).unwrap();
        let mut expected: Json =
            serde_json::from_str(include_str!("fixtures/specs/schema-lookup.json")).unwrap();
        for source in expected["source_reads"].as_array_mut().unwrap() {
            source["snapshots_created"] = json!(created);
        }
        assert_eq!(actual, expected);
    }
    assert_eq!(host.requests, ["DM", "AE", "MEDDRA", "DM", "AE", "MEDDRA"]);
    assert_eq!(host.reads, 3);
    assert_eq!(
        host.published,
        vec![std::fs::read(case.join("expected/adsl.csv")).unwrap(); 2]
    );
}

#[test]
fn original_lookup_failures_match_independent_diagnostics_and_completed_observations() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        case: std::path::PathBuf,
        requests: Vec<String>,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.requests.len()
        }
        fn capture(&mut self, source: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            self.requests.push(source.name.clone());
            Ok(Arc::from(
                std::fs::read(self.case.join(&source.path)).unwrap(),
            ))
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed lookup published")
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-lookup");
    let schema = schema(&root.join("yaml"));
    let original = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let cases: Vec<Json> =
        serde_json::from_str(include_str!("fixtures/specs/lookup-failures.json")).unwrap();
    for variant in cases {
        let raw = original.replace(
            variant["before"].as_str().unwrap(),
            variant["after"].as_str().unwrap(),
        );
        let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
        let mut host = Host {
            case: case.clone(),
            requests: vec![],
        };
        let attempt = run.execute_with_port(&mut host);
        let actual = specification_report::complete(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "schema-lookup",
                specification: "spec.yaml",
                base_directory: ".",
            },
            &mut host,
        )
        .unwrap();
        let mut expected: Json =
            serde_json::from_str(include_str!("fixtures/specs/schema-lookup.json")).unwrap();
        expected["outcome"] = json!("failure");
        expected["nodes"][0]["outcome"] = json!("failure");
        for field in ["diagnostics", "handler_counts"] {
            expected[field] = variant[field].clone();
            expected["nodes"][0][field] = variant[field].clone();
        }
        expected["artifacts"] = json!([]);
        expected["verifications"] = json!([]);
        expected["tables"]
            .as_array_mut()
            .unwrap()
            .retain(|table| table["stage"] == "source");
        assert_eq!(actual, expected, "{}", variant["name"]);
        assert_eq!(host.requests, ["DM", "AE", "MEDDRA"]);
    }
}

#[test]
fn directly_admitted_intermediate_order_terms_keep_omitted_schema_defaults() {
    use yamaa_core::{
        schema::{DocumentLimits, ValidationBudget},
        specification::PreparedSpecification,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-lookup");
    let raw = std::fs::read_to_string(case.join("spec.yaml"))
        .unwrap()
        .replace(
            "order_by: [AE.AEDY]",
            "order_by: [{variable: AE.AEDY, direction: desc, nulls: first}]",
        );
    let normalized = prepare(&schema(&root.join("yaml")), raw.as_bytes());
    for (omit_direction, omit_nulls) in [(true, true), (true, false), (false, true), (false, false)]
    {
        let input = normalized.model().document();
        let selection = sequence(input, input.field(input.root(), "intermediates").unwrap())[0];
        let term = sequence(input, input.field(selection, "order_by").unwrap())[0];
        let mut nodes = input.nodes().to_vec();
        let N::Mapping(fields) = &input.nodes()[term] else {
            panic!("mapping order term")
        };
        nodes[term] = N::Mapping(
            fields
                .iter()
                .copied()
                .filter(|&(name, _)| {
                    !((omit_direction && text(input, name) == "direction")
                        || (omit_nulls && text(input, name) == "nulls"))
                })
                .collect(),
        );
        fn copy_node(input: &[N], id: usize, output: &mut Vec<N>) -> usize {
            let node = match &input[id] {
                N::Sequence(items) => N::Sequence(
                    items
                        .iter()
                        .map(|&id| copy_node(input, id, output))
                        .collect(),
                ),
                N::Mapping(items) => N::Mapping(
                    items
                        .iter()
                        .map(|&(a, b)| (copy_node(input, a, output), copy_node(input, b, output)))
                        .collect(),
                ),
                node => node.clone(),
            };
            let id = output.len();
            output.push(node);
            id
        }
        let mut reachable = Vec::new();
        let root = copy_node(&nodes, input.root(), &mut reachable);
        let document = Document::new(reachable, root, DocumentLimits::default()).unwrap();
        let model =
            SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
                .unwrap()
                .unwrap();
        let prepared = PreparedSpecification::prepare(&model).unwrap();
        let tables = prepared
            .sources()
            .iter()
            .map(|source| {
                csv_source::parse_text_table(
                    &std::fs::read(case.join(&source.path)).unwrap(),
                    Default::default(),
                    TableLimits {
                        max_rows: 100,
                        max_columns: 64,
                        max_cells: 6400,
                        max_batches: 1,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let schemas = tables.iter().map(TableAccess::schema).collect::<Vec<_>>();
        let plan = prepared.bind_sources(&schemas).unwrap();
        let order = &plan.intermediates()[0].selection.as_ref().unwrap().order_by[0];
        assert_eq!(order.descending, !omit_direction);
        assert_eq!(order.nulls_first, !omit_nulls);
    }
}

#[test]
fn single_buffer_execution_reports_source_count_without_an_internal_failure() {
    use yamaa_adapters::{specification_diagnostics, specification_run::PreparedRun};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-lookup");
    let run = PreparedRun::prepare(prepare(
        &schema(&root.join("yaml")),
        &std::fs::read(case.join("spec.yaml")).unwrap(),
    ))
    .unwrap();
    let error = run
        .execute_csv(&std::fs::read(case.join("input/dm.csv")).unwrap())
        .err()
        .unwrap();
    let actual: serde_json::Value = serde_json::from_str(&specification_diagnostics::failure(
        &error,
        Some(run.source()),
    ))
    .unwrap();
    assert_eq!(
        actual,
        serde_json::json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"bind","code":"source_count"}})
    );
}

#[test]
fn original_window_yaml_compiles_and_publishes_complete_independent_report() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
        requests: usize,
        published: Vec<Vec<u8>>,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!((&*source.name, &*source.path), ("VS", "input/vs.csv"));
            assert!(self.bytes.len() <= maximum);
            self.requests += 1;
            self.reads = 1;
            Ok(self.bytes.clone())
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            assert_eq!(path, "advs.csv");
            self.published.push(bytes.to_vec());
            Ok(())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-window-functions");
    let prepared = prepare(
        &schema(&root.join("yaml")),
        &std::fs::read(case.join("spec.yaml")).unwrap(),
    );
    assert!(
        !prepared.windows().is_empty(),
        "shared admission expands named windows"
    );
    let run = PreparedRun::prepare(prepared).unwrap();
    let mut host = Host {
        bytes: Arc::from(std::fs::read(case.join("input/vs.csv")).unwrap()),
        reads: 0,
        requests: 0,
        published: vec![],
    };
    for created in [1, 0] {
        let attempt = run.execute_with_port(&mut host);
        let report = specification_report::complete(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "schema-window-functions",
                specification: "spec.yaml",
                base_directory: ".",
            },
            &mut host,
        )
        .unwrap();
        let mut expected: Json =
            serde_json::from_str(include_str!("fixtures/specs/schema-window-functions.json"))
                .unwrap();
        expected["source_reads"][0]["snapshots_created"] = json!(created);
        assert_eq!(report, expected);
    }
    assert_eq!((host.reads, host.requests), (1, 2));
    assert_eq!(
        host.published,
        vec![std::fs::read(case.join("expected/advs.csv")).unwrap(); 2]
    );
}

#[test]
fn original_window_failures_match_independent_diagnostics_and_completed_observations() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        requests: Vec<String>,
    }
    impl SourcePort for Host {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.requests.len()
        }
        fn capture(&mut self, source: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            self.requests.push(source.name.clone());
            assert_eq!(source.path, "input/vs.csv");
            Ok(self.bytes.clone())
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed window published")
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-window-functions");
    let schema = schema(&root.join("yaml"));
    let original = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let cases: Vec<Json> =
        serde_json::from_str(include_str!("fixtures/specs/window-failures.json")).unwrap();
    for variant in cases {
        let raw = original.replace(
            variant["before"].as_str().unwrap(),
            variant["after"].as_str().unwrap(),
        );
        let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
        let mut content = std::fs::read_to_string(case.join("input/vs.csv")).unwrap();
        if let Some(input_before) = variant["input_before"].as_str() {
            content = content.replace(input_before, variant["input_after"].as_str().unwrap());
        }
        let mut host = Host {
            bytes: Arc::from(content.into_bytes()),
            requests: vec![],
        };
        let attempt = run.execute_with_port(&mut host);
        let actual = specification_report::complete(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "schema-window-functions",
                specification: "spec.yaml",
                base_directory: ".",
            },
            &mut host,
        )
        .unwrap();
        let mut expected: Json =
            serde_json::from_str(include_str!("fixtures/specs/schema-window-functions.json"))
                .unwrap();
        expected["outcome"] = json!("failure");
        expected["nodes"][0]["outcome"] = json!("failure");
        for field in ["diagnostics", "handler_counts"] {
            expected[field] = variant[field].clone();
            expected["nodes"][0][field] = variant[field].clone();
        }
        expected["artifacts"] = json!([]);
        expected["verifications"] = json!([]);
        expected["tables"]
            .as_array_mut()
            .unwrap()
            .retain(|table| table["stage"] == "source");
        if let Some(cells) = variant["source_cells"].as_array() {
            for cell in cells {
                expected["tables"][0]["rows"][cell[0].as_u64().unwrap() as usize]
                    [cell[1].as_u64().unwrap() as usize] = cell[2].clone();
            }
        }
        assert_eq!(actual, expected, "{}", variant["name"]);
        assert_eq!(host.requests, ["VS"]);
    }
}

#[test]
fn inherited_original_preparation_prunes_before_source_binding() {
    use yamaa_core::schema::{NormalizationBudget, SchemaSource};
    use yamaa_engine::{inheritance as graph, inheritance_preparation as lifecycle};
    struct Host {
        root: std::path::PathBuf,
        reads: Vec<String>,
        resolutions: Vec<String>,
        paths: Vec<String>,
        fail_path: bool,
    }
    impl graph::SourcePort for Host {
        type Error = &'static str;
        fn canonicalize(
            &mut self,
            _: &str,
            written: &str,
        ) -> Result<graph::Source, graph::SourceError<&'static str>> {
            self.resolutions.push(written.into());
            Ok(graph::Source {
                identity: written.into(),
                display_path: written.into(),
            })
        }
        fn read(
            &mut self,
            source: &graph::Source,
        ) -> Result<Document, graph::SourceError<&'static str>> {
            self.reads.push(source.identity.clone());
            Ok(decode_yaml(
                &std::fs::read(self.root.join(&source.identity)).unwrap(),
                Default::default(),
            )
            .unwrap()
            .document)
        }
    }
    impl lifecycle::PathPort for Host {
        fn rebase(
            &mut self,
            _: &graph::Source,
            _: &graph::Source,
            written: &str,
            _: usize,
        ) -> Result<String, &'static str> {
            if self.fail_path {
                return Err("original path failure");
            }
            self.paths.push(written.into());
            Ok(written.into())
        }
    }
    impl yamaa_adapters::specification_source::InheritancePort for Host {
        type Error = &'static str;
        fn canonicalize(
            &mut self,
            declaring: &str,
            written: &str,
        ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
            graph::SourcePort::canonicalize(self, declaring, written)
        }
        fn capture(
            &mut self,
            source: &graph::Source,
            maximum: usize,
        ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
            self.reads.push(source.identity.clone());
            let bytes = std::fs::read(self.root.join(&source.identity)).unwrap();
            assert!(bytes.len() <= maximum);
            Ok(bytes)
        }
        fn rebase(
            &mut self,
            layer: &graph::Source,
            entry: &graph::Source,
            written: &str,
            maximum: usize,
        ) -> Result<String, Self::Error> {
            lifecycle::PathPort::rebase(self, layer, entry, written, maximum)
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-inheritance");
    let schema = schema(&root.join("yaml"));
    let raw = decode_yaml(
        &std::fs::read(case.join("spec_study.yaml")).unwrap(),
        Default::default(),
    )
    .unwrap()
    .document;
    let mut host = Host {
        root: case.clone(),
        reads: vec![],
        resolutions: vec![],
        paths: vec![],
        fail_path: false,
    };
    let prepared = lifecycle::prepare(
        schema.structure(),
        graph::Source {
            identity: "spec_study.yaml".into(),
            display_path: "spec_study.yaml".into(),
        },
        raw.clone(),
        &mut host,
        &mut NormalizationBudget::new(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(host.reads, ["spec_organization.yaml", "spec_compound.yaml"]);
    assert_eq!(
        host.resolutions,
        [
            "spec_organization.yaml",
            "spec_compound.yaml",
            "spec_organization.yaml"
        ]
    );
    assert_eq!(
        prepared
            .layers()
            .iter()
            .map(|layer| layer.source.identity.as_str())
            .collect::<Vec<_>>(),
        [
            "spec_organization.yaml",
            "spec_compound.yaml",
            "spec_study.yaml"
        ]
    );
    let d = prepared.model().document();
    let columns = sequence(d, d.field(d.root(), "columns").unwrap());
    assert_eq!(
        columns
            .iter()
            .map(|&column| text(d, d.field(column, "name").unwrap()))
            .collect::<Vec<_>>(),
        ["USUBJID", "PARAMCD", "AVAL", "AVALU"]
    );
    assert_eq!(
        text(d, d.field(columns[2], "label").unwrap()),
        "Analysis Value"
    );
    assert_eq!(
        text(d, d.field(columns[3], "label").unwrap()),
        "Standardized Analysis Unit"
    );
    let compiled =
        yamaa_core::specification::PreparedSpecification::prepare(prepared.model()).unwrap();
    assert_eq!(compiled.sources().len(), 1);
    assert_eq!(
        (&*compiled.source().name, &*compiled.source().path),
        ("LB", "input/lb.csv")
    );
    assert_eq!(prepared.origins().len(), d.nodes().len());
    for origin in prepared.origins() {
        if origin.source == SchemaSource::Input {
            assert!(origin.node < prepared.normalization_input().nodes().len());
        }
    }
    assert!(prepared
        .provenance()
        .iter()
        .any(|origin| origin.path == "columns.AVAL.label" && origin.layer == 2));

    let table = yamaa_adapters::typed_csv::parse(
        &std::fs::read(case.join("input/lb.csv")).unwrap(),
        &compiled.source().types,
        Default::default(),
        TableLimits {
            max_rows: 100,
            max_columns: 64,
            max_batches: 1,
            max_cells: 6400,
        },
    )
    .unwrap();
    let plan = compiled.bind(table.schema()).unwrap();
    let execution = plan.execute_observed(&table, limits()).result.unwrap();
    assert_eq!(
        execution.dataset.rows(),
        &[
            vec![
                Value::Str("01".into()),
                Value::Str("ALT".into()),
                Value::float(12.5),
                Value::Str("U/L".into())
            ],
            vec![
                Value::Str("02".into()),
                Value::Str("ALT".into()),
                Value::float(21.0),
                Value::Str("U/L".into())
            ],
        ]
    );
    assert_eq!(execution.verifications.len(), 1);
    assert_eq!(execution.verifications[0].failed_count, 0);
    assert_eq!(
        yamaa_adapters::csv_artifact::render(&execution.dataset, &[0, 1, 2, 3], 1024).unwrap(),
        std::fs::read(case.join("expected/adlb.csv")).unwrap()
    );
    for (original, reserved, path) in [
        ("scope", "label", "metadata.label"),
        ("analysis_role", "origin", "columns.AVAL.metadata.origin"),
    ] {
        let mut nodes = d.nodes().to_vec();
        let key = nodes
            .iter()
            .position(|n| matches!(n,N::Text(value) if value == original))
            .unwrap();
        nodes[key] = N::Text(reserved.into());
        let changed = SpecificationDocument::admit(
            Document::new(nodes, d.root(), Default::default()).unwrap(),
            &mut yamaa_core::schema::ValidationBudget::new(Default::default()),
        )
        .unwrap()
        .unwrap();
        let error =
            yamaa_core::specification::PreparedSpecification::prepare(&changed).unwrap_err();
        assert!(
            matches!(error,yamaa_core::specification::PrepareError::Unsupported(ref items) if items.iter().any(|item|item.operation == "reserved_metadata_key" && item.path == path))
        );
    }
    host.reads.clear();
    let captured = schema
        .prepare_inherited(
            Source {
                identity: "spec_study.yaml".into(),
                bytes: std::fs::read(case.join("spec_study.yaml")).unwrap(),
            },
            "spec_study.yaml".into(),
            &mut host,
        )
        .unwrap();
    assert_eq!(host.reads, ["spec_organization.yaml", "spec_compound.yaml"]);
    assert_eq!(captured.parents().len(), 2);
    for parent in captured.parents() {
        assert_eq!(
            parent.source().bytes,
            std::fs::read(case.join(&parent.source().identity)).unwrap()
        );
        assert!(!parent.raw().locations.is_empty());
    }
    assert!(captured.inheritance().is_some());
    let run = yamaa_adapters::specification_run::PreparedRun::prepare(captured).unwrap();
    struct Data {
        bytes: Arc<[u8]>,
        reads: usize,
        published: Vec<Vec<u8>>,
    }
    impl yamaa_adapters::specification_run::SourcePort for Data {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &yamaa_core::specification::SourceDeclaration,
            maximum: usize,
        ) -> Result<Arc<[u8]>, ()> {
            assert_eq!((&*source.name, &*source.path), ("LB", "input/lb.csv"));
            assert!(self.bytes.len() <= maximum);
            self.reads = 1;
            Ok(self.bytes.clone())
        }
    }
    impl yamaa_adapters::specification_report::ArtifactPort for Data {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            assert_eq!(path, "adlb.csv");
            self.published.push(bytes.to_vec());
            Ok(())
        }
    }
    let mut data = Data {
        bytes: Arc::from(std::fs::read(case.join("input/lb.csv")).unwrap()),
        reads: 0,
        published: vec![],
    };
    for created in [1, 0] {
        let attempt = run.execute_with_port(&mut data);
        let report = yamaa_adapters::specification_report::complete(
            &run,
            &attempt,
            yamaa_adapters::specification_report::Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "schema-inheritance",
                specification: "spec_study.yaml",
                base_directory: ".",
            },
            &mut data,
        )
        .unwrap();
        let mut expected: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/specs/schema-inheritance.json")).unwrap();
        expected["source_reads"][0]["snapshots_created"] = serde_json::json!(created);
        assert_eq!(report, expected);
    }
    assert_eq!(
        data.published,
        vec![std::fs::read(case.join("expected/adlb.csv")).unwrap(); 2]
    );
    for fail_path in [false, true] {
        host.fail_path = fail_path;
        let error = lifecycle::prepare(
            schema.structure(),
            graph::Source {
                identity: "spec_study.yaml".into(),
                display_path: "spec_study.yaml".into(),
            },
            raw.clone(),
            &mut host,
            &mut NormalizationBudget::new(Default::default()),
            lifecycle::Limits {
                rebased_bytes: 0,
                ..Default::default()
            },
        )
        .unwrap_err();
        if fail_path {
            assert!(matches!(
                error,
                lifecycle::Error::Path("original path failure")
            ));
        } else {
            assert!(matches!(error, lifecycle::Error::PathBytes));
        }
    }
}

#[test]
fn classified_capture_failures_retain_complete_reports_without_publication() {
    use serde_json::json;
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PortError, PreparedRun, SourcePort},
    };
    use yamaa_core::resource::ResourceFailure;
    use yamaa_engine::specification::SourceDeclaration;
    struct Payload {
        cause: Option<ResourceFailure>,
        identity: Arc<()>,
    }
    struct Port {
        bytes: Arc<[u8]>,
        calls: Vec<String>,
        fail_at: usize,
        reads: usize,
        cached: bool,
        cause: Option<ResourceFailure>,
        identity: Arc<()>,
    }
    impl SourcePort for Port {
        type Error = Payload;
        fn resource_failure(&self, error: &Payload) -> Option<ResourceFailure> {
            error.cause
        }
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            limit: usize,
        ) -> Result<Arc<[u8]>, Payload> {
            let index = self.calls.len();
            self.calls.push(source.name.clone());
            if index == self.fail_at {
                return Err(Payload {
                    cause: self.cause,
                    identity: Arc::clone(&self.identity),
                });
            }
            assert_eq!(source.name, "DM");
            assert!(self.bytes.len() <= limit);
            self.reads += usize::from(!self.cached);
            Ok(Arc::clone(&self.bytes))
        }
    }
    struct NoPublication;
    impl specification_report::ArtifactPort for NoPublication {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed build published")
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-lookup");
    let schema = schema(&root.join("yaml"));
    let run = PreparedRun::prepare(prepare(
        &schema,
        &std::fs::read(case.join("spec.yaml")).unwrap(),
    ))
    .unwrap();
    let bytes: Arc<[u8]> = std::fs::read(case.join("input/dm.csv")).unwrap().into();
    let metadata = || Identity {
        runtime: "python",
        runtime_version: "test",
        engine_version: "test",
        example: "schema-lookup",
        specification: "spec.yaml",
        base_directory: ".",
    };
    for (cause, condition) in [
        (ResourceFailure::Missing, "resource_path_missing"),
        (
            ResourceFailure::NotRegularFile,
            "resource_path_not_regular_file",
        ),
    ] {
        for fail_at in [0, 1] {
            for cached in [false, true] {
                let identity = Arc::new(());
                let mut port = Port {
                    bytes: Arc::clone(&bytes),
                    calls: vec![],
                    fail_at,
                    reads: 0,
                    cached,
                    cause: Some(cause),
                    identity: Arc::clone(&identity),
                };
                let mut attempt = run.execute_with_port(&mut port);
                assert_eq!(
                    port.calls,
                    if fail_at == 0 {
                        vec!["DM"]
                    } else {
                        vec!["DM", "AE"]
                    }
                );
                let Err(PortError::Capture(error)) = &attempt.result else {
                    panic!("capture failure missing")
                };
                assert!(Arc::ptr_eq(&identity, &error.identity));
                if fail_at == 1 {
                    assert!(attempt.sources[0].table.is_some());
                }
                let (dataset, path) = if fail_at == 0 {
                    ("DM", "input/dm.csv")
                } else {
                    ("AE", "input/ae.csv")
                };
                let diagnostics = json!([{"phase":"validation","condition":condition,"requirement":"REQ-0785","spec_paths":[format!("input.{dataset}.path")],"context":{"dataset":dataset,"path":path}}]);
                let mut reads = vec![];
                if fail_at == 1 {
                    reads.push(json!({"base_directory":".","path":"input/dm.csv","outcome":"captured","condition":null,"snapshots_created":usize::from(!cached)}));
                }
                reads.push(json!({"base_directory":".","path":path,"outcome":"failure","condition":condition,"snapshots_created":0}));
                let expected = json!({"report_version":"0.3.0-draft","runtime":"python","backend":"rust","runtime_version":"test","engine_version":"test","example":"schema-lookup","outcome":"failure","artifacts":[],"diagnostics":diagnostics,"unsupported":[],"handler_counts":[],"nodes":[{"specification":"spec.yaml","outcome":"failure","diagnostics":diagnostics,"unsupported":[],"handler_counts":[]}],"tables":[],"verifications":[],"callbacks":[],"source_reads":reads,"error":null});
                let result =
                    specification_report::build_result(&run, &attempt, metadata()).unwrap();
                assert_eq!(result.observations(), expected);
                assert!(result.output().is_none());
                assert!(matches!(
                    result.save(&mut NoPublication),
                    Err(yamaa_engine::specification_output::SaveError::FailedBuild)
                ));
                // Failed accounting and opaque host errors cannot borrow a known
                // finding left in the source ledger to become semantic failures.
                attempt.sources.last_mut().unwrap().read.snapshots_created = None;
                assert!(specification_report::failure(&run, &attempt, metadata()).is_err());
                attempt.sources.last_mut().unwrap().read.snapshots_created = Some(0);
                attempt.sources.last_mut().unwrap().read.failure = None;
                assert!(specification_report::failure(&run, &attempt, metadata()).is_err());
            }
        }
    }
    let identity = Arc::new(());
    let mut port = Port {
        bytes,
        calls: vec![],
        fail_at: 0,
        reads: 0,
        cached: false,
        cause: None,
        identity: Arc::clone(&identity),
    };
    let attempt = run.execute_with_port(&mut port);
    let Err(PortError::Capture(error)) = &attempt.result else {
        panic!("opaque failure missing")
    };
    assert!(Arc::ptr_eq(&identity, &error.identity));
    assert_eq!(attempt.sources[0].read.failure, None);
    assert!(specification_report::build_result(&run, &attempt, metadata()).is_err());
}

#[test]
fn resource_findings_use_written_inherited_paths_not_rebased_locations() {
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
        specification_source::{InheritancePort, Source},
    };
    use yamaa_core::resource::ResourceFailure;
    use yamaa_engine::{
        inheritance::{Source as FileIdentity, SourceError},
        specification::SourceDeclaration,
    };
    struct Files {
        parent: Vec<u8>,
    }
    impl InheritancePort for Files {
        type Error = ();
        fn canonicalize(
            &mut self,
            _: &str,
            written: &str,
        ) -> Result<FileIdentity, SourceError<()>> {
            assert_eq!(written, "parent.yaml");
            Ok(FileIdentity {
                identity: written.into(),
                display_path: written.into(),
            })
        }
        fn capture(&mut self, _: &FileIdentity, _: usize) -> Result<Vec<u8>, SourceError<()>> {
            Ok(self.parent.clone())
        }
        fn rebase(
            &mut self,
            layer: &FileIdentity,
            _: &FileIdentity,
            written: &str,
            _: usize,
        ) -> Result<String, ()> {
            Ok(if layer.identity == "parent.yaml" {
                format!("internal-location/{written}")
            } else {
                written.into()
            })
        }
    }
    struct Missing;
    impl SourcePort for Missing {
        type Error = ();
        fn resource_failure(&self, _: &()) -> Option<ResourceFailure> {
            Some(ResourceFailure::Missing)
        }
        fn capture_reads(&self) -> usize {
            0
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            Err(())
        }
    }
    for declaration in [
        "SRC: nested/data.csv",
        "SRC: {path: nested/data.csv, types: {ID: int}}",
    ] {
        for child_path in [None, Some("child.csv")] {
            let mut entry = String::from("schema_version: '1.0'\nparents: parent.yaml\ndomain: TEST\nkeys: [ID]\noutput: {path: result.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: {source: SRC.ID}}]\n");
            if let Some(path) = child_path {
                entry.push_str(&format!("input: {{SRC: {{path: {path}}}}}\n"));
            }
            let parent = format!("schema_version: '1.0'\ninput: {{{declaration}}}\n");
            let document = yamaa_adapters::shipped_schema::prepare(
                Source {
                    identity: "entry.yaml".into(),
                    bytes: entry.into_bytes(),
                },
                "entry.yaml".into(),
                &mut Files {
                    parent: parent.into_bytes(),
                },
            )
            .unwrap();
            let written = child_path.unwrap_or("nested/data.csv");
            assert_eq!(document.written_source_path("SRC"), Some(written));
            assert_eq!(document.written_source_path("ABSENT"), None);
            let run = PreparedRun::prepare(document).unwrap();
            if child_path.is_none() {
                assert_eq!(run.source().path, "internal-location/nested/data.csv");
            }
            let attempt = run.execute_with_port(&mut Missing);
            let report = specification_report::failure(
                &run,
                &attempt,
                Identity {
                    runtime: "r",
                    runtime_version: "test",
                    engine_version: "test",
                    example: "inheritance",
                    specification: "entry.yaml",
                    base_directory: ".",
                },
            )
            .unwrap();
            assert_eq!(
                report["diagnostics"],
                serde_json::json!([{"phase":"validation","condition":"resource_path_missing","requirement":"REQ-0785","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":written}}])
            );
        }
    }
}

#[test]
fn original_column_literals_match_complete_reference_reports_and_csv() {
    independent_scalar_reports(include_str!("fixtures/column_literals.tsv"), "literal", 4);
}

#[test]
fn original_conversion_handlers_match_complete_reference_reports_and_csv() {
    independent_scalar_reports(
        include_str!("fixtures/original_conversion_handlers.tsv"),
        "handler",
        5,
    );
}

#[test]
fn original_row_conversion_handlers_match_complete_reference_reports_and_csv() {
    independent_scalar_reports(
        include_str!("fixtures/original_row_conversion_handlers.tsv"),
        "row-handler",
        8,
    );
}

fn independent_scalar_reports(fixture: &str, prefix: &str, expected_cases: usize) {
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        reads: usize,
        saves: usize,
        expected: Vec<u8>,
    }
    impl SourcePort for Port {
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            assert!(maximum >= 5);
            self.reads += 1;
            Ok(Arc::from(&b"ID\n1\n"[..]))
        }
    }
    impl ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), ()> {
            assert!(!self.expected.is_empty(), "failed literal build published");
            assert_eq!(path, "result.csv");
            assert_eq!(content, self.expected);
            self.saves += 1;
            Ok(())
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut cases = 0;
    for row in fixture.lines().skip(1) {
        let fields = row.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 4);
        let bytes = (0..fields[3].len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&fields[3][i..i + 2], 16).unwrap())
            .collect();
        let mut port = Port {
            reads: 0,
            saves: 0,
            expected: bytes,
        };
        let run = PreparedRun::prepare(prepare(&schema, fields[1].as_bytes())).unwrap();
        let attempt = run.execute_with_port(&mut port);
        let example = format!("{prefix}-{}", fields[0]);
        let result = specification_report::build_result(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: &example,
                specification: "spec.yaml",
                base_directory: ".",
            },
        )
        .unwrap();
        drop(run);
        drop(attempt);
        let expected: serde_json::Value = serde_json::from_str(fields[2]).unwrap();
        let mut unsaved = expected.clone();
        unsaved["artifacts"] = serde_json::json!([]);
        assert_eq!(result.observations(), unsaved, "{}", fields[0]);
        if port.expected.is_empty() {
            assert!(result.output().is_none());
            for _ in 0..2 {
                assert!(matches!(
                    result.save(&mut port),
                    Err(yamaa_engine::specification_output::SaveError::FailedBuild)
                ));
            }
            assert_eq!(port.saves, 0);
        } else {
            assert!(result.output().is_some());
            for _ in 0..2 {
                assert_eq!(result.save(&mut port).unwrap(), &expected);
            }
            assert_eq!(port.saves, 2);
        }
        assert_eq!(result.observations(), unsaved);
        assert_eq!(port.reads, 1);
        cases += 1;
    }
    assert_eq!(cases, expected_cases);
}
