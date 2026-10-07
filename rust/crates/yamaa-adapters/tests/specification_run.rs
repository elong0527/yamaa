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
        (format!("{raw}\nmetadata:\n  note: preserved\n"), "metadata"),
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
        assert!(attempt.read.captured);
        assert_eq!(attempt.read.snapshots_created, Some(created));
        assert!(Arc::ptr_eq(attempt.snapshot.as_ref().unwrap(), &content));
        assert_eq!(attempt.table.as_ref().unwrap().row_count(), 2);
        let response = attempt.result.unwrap();
        assert!(response.table.is_none());
        assert!(response.outcome.contains("division_by_zero"));
    }
    assert_eq!(port.requests, 2);
    let invalid = raw.replace("100 * (AVAL - BASE) / BASE", "AVAL +");
    let run = PreparedRun::prepare(prepare(&schema, invalid.as_bytes())).unwrap();
    let attempt = run.execute_with_port(&mut port);
    assert!(attempt.table.is_some());
    assert!(matches!(
        attempt.result,
        Err(PortError::Run(Error::Bind(_)))
    ));
    port.content = Arc::from(b"A\n\"bad".as_slice());
    let attempt = run.execute_with_port(&mut port);
    assert!(attempt.snapshot.is_some());
    assert!(attempt.table.is_none());
    assert!(matches!(
        attempt.result,
        Err(PortError::Run(Error::Source(_)))
    ));
    port.fail = true;
    let attempt = run.execute_with_port(&mut port);
    assert!(!attempt.read.captured);
    assert_eq!(attempt.read.snapshots_created, Some(0));
    assert!(attempt.snapshot.is_none() && attempt.table.is_none());
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
    for name in ["negative-zero-division", "negative-integer-overflow"] {
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
                    .join("tests/fixtures/specifications")
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
