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

use yamaa_core::dataset::{self, DatasetPlan};

/// Rows of a converted readable JSON fixture (formerly hex-encoded TSV).
pub(crate) fn fixture_rows(truth: &str) -> Vec<serde_json::Value> {
    serde_json::from_str(truth).unwrap()
}

/// Exact bytes of a fixture text cell; the two non-UTF-8 cells in the whole
/// fixture set are tagged as {"hex": "..."} objects instead of strings.
pub(crate) fn fixture_bytes(value: &serde_json::Value) -> Vec<u8> {
    if let Some(text) = value.as_str() {
        return text.as_bytes().to_vec();
    }
    let hex = value["hex"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

pub(crate) fn spec_findings(
    error: &yamaa_adapters::specification_run::Error,
    source: Option<&yamaa_core::specification::SourceDeclaration>,
) -> Option<Vec<serde_json::Value>> {
    yamaa_adapters::specification_diagnostics::findings_with_limit(
        error,
        source,
        yamaa_core::diagnostic::MAX_PROJECTION_BYTES,
    )
    .ok()
    .flatten()
}

pub(crate) fn text(d: &Document, id: usize) -> &str {
    let N::Text(value) = &d.nodes()[id] else {
        panic!("expected text")
    };
    value
}
pub(crate) fn sequence(d: &Document, id: usize) -> &[usize] {
    let N::Sequence(value) = &d.nodes()[id] else {
        panic!("expected sequence")
    };
    value
}
pub(crate) fn schema(root: &Path) -> Arc<CapturedSchema> {
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
pub(crate) fn prepare(schema: &Arc<CapturedSchema>, source: &[u8]) -> PreparedDocument {
    schema
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: source.to_vec(),
        })
        .unwrap()
}
pub(crate) fn lower(spec: &SpecificationDocument, source: &TableSchema) -> DatasetPlan {
    yamaa_core::specification::PreparedSpecification::prepare(spec)
        .unwrap()
        .bind(source)
        .unwrap()
}
pub(crate) fn limits() -> dataset::Limits {
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
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
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
    use yamaa_core::specification::{PrepareError, PreparedSpecification};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let raw =
        std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap();
    let written = format!("{raw}\nmetadata:\n  label: reserved\n");
    let captured = prepare(&schema, written.as_bytes());
    let error = PreparedSpecification::prepare(captured.model()).unwrap_err();
    let PrepareError::Invalid(findings) = error else {
        panic!("{error:?}")
    };
    assert!(
        matches!(&findings[0], yamaa_core::specification::PreflightFinding::Scalar(diagnostic) if diagnostic.code == yamaa_core::diagnostic::ConditionCode::SubmissionReservedMetadata && diagnostic.spec_paths == ["metadata.label"])
    );
    let written = raw.replace("100 * (AVAL - BASE) / BASE", "LB.LBSTRESN + 1");
    let captured = prepare(&schema, written.as_bytes());
    let prepared = PreparedSpecification::prepare(captured.model()).unwrap();
    let schema = TableSchema::new(vec![]).unwrap();
    assert!(matches!(
        prepared.bind(&schema),
        Err(yamaa_core::specification::BindError::Invalid(_))
    ));
}

#[test]
fn binding_uses_shared_reference_and_dependency_findings() {
    use yamaa_core::reference_binding::Diagnostic;
    use yamaa_core::specification::{BindError, BindFinding, PreparedSpecification};
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
        let mut context = json!({"expr":expr});
        if condition == "integer_overflow" {
            context["minimum"] = json!(i64::MIN);
            context["maximum"] = json!(i64::MAX);
            context["value"] = json!("9223372036854776832");
        }
        let expected = json!({"protocol":"execution/1","outcome":{
            "status":"condition","diagnostic":{"phase":"derivation","condition":condition,"requirement":requirement,
            "spec_paths":[format!("columns.{column}.derivation.compute")],"context":context},
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
    use yamaa_core::specification::{BindError, BindFinding};
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
    use yamaa_adapters::specification_run::PreparedRun;
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
            Some("REQ-0106"),
            json!({"identifier":"BOGUS"}),
        ),
        (
            "LBSTRESN + 1",
            "unresolvable_name",
            Some("REQ-0106"),
            json!({"identifier":"LBSTRESN","suggestion":"LB.LBSTRESN"}),
        ),
    ] {
        let written = raw.replace("100 * (AVAL - BASE) / BASE", expr);
        let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
        let error = run.execute_csv(&data).err().unwrap();
        assert_eq!(
            spec_findings(&error, Some(run.source())).unwrap(),
            vec![
                json!({"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":["columns.PCHG.derivation.compute.expr"],"context":context})
            ]
        );
    }
    let written = raw.replace("derivation: LB.LBSTRESN", "derivation: LB.BOGUS");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&data).err().unwrap();
    assert_eq!(
        spec_findings(&error, Some(run.source())).unwrap(),
        vec![
            json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.AVAL.derivation.source"],"context":{"identifier":"LB.BOGUS"}})
        ]
    );
    let written = raw.replace("100 * (AVAL - BASE) / BASE", "AVAL +");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(b"A\n\"bad").err().unwrap();
    assert_eq!(
        spec_findings(&error, Some(run.source())).unwrap(),
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
    use yamaa_adapters::specification_run::PreparedRun;
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
        spec_findings(&error, None).unwrap(),
        vec![
            json!({"phase":"validation","condition":"missing_derivation","spec_paths":["columns.PCHG.derivation"],"requirement":"REQ-0198","context":{"column":"PCHG"}}),
            json!({"phase":"validation","condition":"duplicate_identifier","spec_paths":["input.LB","domain"],"requirement":"REQ-0080","context":{"identifier":"LB"}}),
            json!({"phase":"validation","condition":"undeclared_column","spec_paths":["keys[3]"],"requirement":"REQ-0220","context":{"column":"BOGUS"}}),
            json!({"phase":"validation","condition":"driver_unavailable","spec_paths":["base"],"requirement":"REQ-0064","context":{"dataset":"BAD"}}),
        ]
    );
}

#[test]
fn core_preflight_matches_independent_complete_findings() {
    use serde_json::Value;
    use yamaa_adapters::{specification_diagnostics, specification_run::PreparedRun};
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in include_str!("../fixtures/preflight.tsv").lines().skip(1) {
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
        include_str!("../fixtures/output_declarations.tsv"),
        "output",
        (5, 8),
        b"ID\n1\n",
    );
}

#[test]
fn core_grammar_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("../fixtures/grammar_diagnostics.tsv"),
        "grammar",
        (7, 7),
        b"ID\n1\n",
    );
}

#[test]
fn core_binding_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("../fixtures/binding_diagnostics.tsv"),
        "binding",
        (10, 11),
        b"ID,V\n1,2\n",
    );
}

#[test]
fn core_csv_profile_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("../fixtures/csv_profile_diagnostics.json"),
        "csv",
        (13, 13),
        b"",
    );
}

#[test]
fn core_window_findings_match_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("../fixtures/window_diagnostics.tsv"),
        "window",
        (3, 3),
        b"ID,V\n1,2\n",
    );
}

#[test]
fn core_predicate_matches_independent_complete_failed_report() {
    independent_failed_reports(
        include_str!("../fixtures/predicate_diagnostics.tsv"),
        "predicate",
        (1, 1),
        b"ID,V\n1,2\n",
    );
}

#[test]
fn core_source_typing_matches_independent_complete_failed_reports() {
    independent_failed_reports(
        include_str!("../fixtures/source_typing.json"),
        "typing",
        (4, 4),
        b"",
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
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
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
    // Converted fixtures are JSON rows; the remaining readable TSV fixtures
    // keep their original three/four-column shape. Both normalize to
    // (case, source, expected, optional exact source bytes).
    let rows: Vec<(String, String, Value, Option<Vec<u8>>)> = if fixture.starts_with('[') {
        fixture_rows(fixture)
            .iter()
            .map(|row| {
                (
                    row["case"].as_str().unwrap().to_owned(),
                    row["source"].as_str().unwrap().to_owned(),
                    row["expected"].clone(),
                    row.get("source_bytes").map(fixture_bytes),
                )
            })
            .collect()
    } else {
        fixture
            .lines()
            .skip(1)
            .map(|line| {
                let fields = line.split('\t').collect::<Vec<_>>();
                assert!(matches!(fields.len(), 3 | 4));
                (
                    fields[0].to_owned(),
                    fields[1].to_owned(),
                    serde_json::from_str(fields[2]).unwrap(),
                    (fields.len() == 4)
                        .then(|| fixture_bytes(&serde_json::json!({"hex": fields[3]}))),
                )
            })
            .collect()
    };
    for (case, source, expected, source_bytes) in &rows {
        let run = PreparedRun::prepare(prepare(&schema, source.as_bytes())).unwrap();
        let content = source_bytes.as_deref().unwrap_or(content);
        let mut port = Port(0, Arc::from(content));
        let attempt = run.execute_with_port(&mut port);
        let example = format!("{prefix}-{}", case);
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
        assert_eq!(result.observations(), *expected, "{}", case);
        assert!(result.output().is_none());
        for _ in 0..2 {
            assert!(matches!(
                result.save(&mut port),
                Err(yamaa_core::specification_output::SaveError::FailedBuild)
            ));
            assert_eq!(result.observations(), *expected);
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
    use yamaa_adapters::specification_run::PreparedRun;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let case = root.join("benchmarks/negative-zero-division");
    let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
    let data = std::fs::read(case.join("input/lb.csv")).unwrap();
    let unknown = json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0106","spec_paths":["columns.PCHG.derivation.compute.expr"],"context":{"identifier":"BOGUS"}});
    let cycle = json!({"phase":"validation","condition":"dependency_cycle","requirement":"REQ-0072","spec_paths":["columns.PCHG.derivation.compute"],"context":{"cycle":["PCHG","PCHG"]}});
    let written = raw
        .replace("derivation: LB.LBSTRESN", "derivation: LB.BOGUS")
        .replace("100 * (AVAL - BASE) / BASE", "BOGUS + PCHG");
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&data).err().unwrap();
    assert_eq!(
        spec_findings(&error, Some(run.source())).unwrap(),
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
        spec_findings(&error, Some(run.source())).unwrap(),
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
    use yamaa_core::specification::{CompilationLimits, PrepareError, PreparedSpecification};
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
            matches!(&error, PrepareError::Limit(actual) if actual .resource == resource),
            "{error:?}"
        );
        let error = specification_run::Error::Prepare(error);
        assert!(spec_findings(&error, None).is_none());
        let envelope = serde_json::from_str::<serde_json::Value>(
            &specification_diagnostics::failure(&error, None),
        )
        .unwrap();
        let outcome = &envelope["outcome"];
        assert_eq!(outcome["status"], "rejected");
        assert_eq!(outcome["stage"], "prepare");
        assert_eq!(outcome["code"], "compiler_limit");
        assert_eq!(outcome["resource"], resource);
        let specification_run::Error::Prepare(PrepareError::Limit(facts)) = &error else {
            unreachable!()
        };
        assert_eq!(outcome["limit"], facts.limit);
        assert_eq!(outcome["actual"].as_u64(), facts.actual.map(|v| v as u64));
        assert_eq!(outcome["spec_paths"], serde_json::json!(facts.spec_paths));
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
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        content: Arc<[u8]>,
        reads: usize,
        requests: usize,
        fail: bool,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
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
            // This attempt starts with the central cumulative source allowance;
            // capture retains the same physical bytes for later owned decoding.
            assert_eq!(limit, yamaa_core::capacity::SOURCE_BYTES);
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
    use yamaa_core::specification::SourceDeclaration;
    struct Files {
        base: std::path::PathBuf,
        reads: usize,
    }
    impl SourcePort for Files {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
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
        "negative-source-missing-field",
        "negative-source-trivial-filter",
        "negative-paired-dates",
        "negative-not-missing-age",
        "negative-implausible-age",
        "negative-invalid-sex",
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
        assert_eq!(actual.observations_value(false), expected, "{name}");
    }
}

#[test]
fn declared_source_types_preserve_ingestion_diagnostics_before_formula_errors() {
    use serde_json::json;
    use yamaa_adapters::specification_run::PreparedRun;
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
        spec_findings(&error, Some(run.source())).unwrap(),
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
        spec_findings(&error, Some(run.source())).unwrap(),
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
        yamaa_core::specification::PreparedSpecification::prepare(document.model()).unwrap();
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
    use yamaa_adapters::specification_run::PreparedRun;
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
            spec_findings(&error, Some(run.source())).unwrap(),
            vec![
                json!({"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":["rows[1].derivations.AVAL.aggregate"],"context":context})
            ]
        );
        let invalid_data = String::from_utf8(data.clone())
            .unwrap()
            .replacen(",0.1", ",invalid", 1);
        let error = run.execute_csv(invalid_data.as_bytes()).err().unwrap();
        assert_eq!(
            spec_findings(&error, Some(run.source())).unwrap(),
            vec![
                json!({"phase":"ingest","condition":"field_parse_failed","requirement":"REQ-0536","spec_paths":["input.LB.types.LBSTRESN"],"context":{"dataset":"LB","field":"LBSTRESN","type":"float","value":"invalid"}})
            ]
        );
    }
}

#[test]
fn declared_field_and_total_row_expression_budgets_precede_compiler_ownership() {
    use yamaa_core::specification::{CompilationLimits, PrepareError, PreparedSpecification};
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
        Err(PrepareError::Limit(
            yamaa_core::specification::CompilerLimit {
                resource: "source_fields",
                ..
            }
        ))
    ));
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            document.model(),
            CompilationLimits {
                numeric_bytes: 3,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit(
            yamaa_core::specification::CompilerLimit {
                resource: "numeric_bytes",
                ..
            }
        ))
    ));
}
