//! Original-document compiler and source-port integration contracts.
use std::{path::Path, sync::Arc};
use yamaa_adapters::{
    arrow_table::TableLimits, csv_source, specification_source::Source, yaml_decode::decode_yaml,
};
use yamaa_core::{
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::TableAccess,
    value::Value,
};

use super::specification_run::*;

#[test]
fn original_success_report_follows_actual_atomic_publication_and_output_failures_do_not_publish() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, CompleteError, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Captured {
        bytes: Arc<[u8]>,
        reads: usize,
    }
    impl SourcePort for Captured {
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
        serde_json::from_str(include_str!("../fixtures/specs/adam-adlb-ordered-sum.json")).unwrap();
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
        serde_json::from_str(include_str!("../fixtures/specs/adam-adlb-ordered-sum.json")).unwrap();
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
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
    }
    impl SourcePort for Host {
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
            "row_count: {min: 999999999999999999999, max: 999999999999999999998}",
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
    // Warning-log admission scans all declarations before granting study ports,
    // even when an earlier payload finding remains deferred.
    let missing_log = format!("{written}  - row_count: {{min: 1, severity: warning}}\n");
    assert!(matches!(
        PreparedRun::prepare(prepare(&schema, missing_log.as_bytes())),
        Err(yamaa_adapters::specification_run::Error::Prepare(
            yamaa_core::specification::PrepareError::Invalid(findings)
        )) if findings.iter().any(|finding| finding.diagnostic().definition().condition == "missing_warning_log")
    ));
}

#[test]
fn row_group_declarations_and_shared_scope_keep_authored_diagnostics() {
    use serde_json::json;
    use yamaa_adapters::specification_run::PreparedRun;
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
        assert_eq!(spec_findings(&error,None).unwrap(),json!([{"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":["rows[1].group_by"],"context":context}]).as_array().unwrap().clone());
    }
    let written = raw
        .replace("id: total", "id: total\n    dataset: ABSENT")
        .replace("    group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT]\n", "");
    let error = PreparedRun::prepare(prepare(&schema, written.as_bytes()))
        .err()
        .unwrap();
    assert_eq!(spec_findings(&error,None).unwrap(),json!([{"phase":"validation","condition":"driver_unavailable","requirement":"REQ-0034","spec_paths":["rows[1].dataset"],"context":{"row":"total","dataset":"ABSENT"}}]).as_array().unwrap().clone());
    let written = raw.replace(
        "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT]",
        "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT, LB.ABSENT]",
    );
    let run = PreparedRun::prepare(prepare(&schema, written.as_bytes())).unwrap();
    let error = run.execute_csv(&bytes).err().unwrap();
    assert_eq!(
        spec_findings(&error, Some(run.source())).unwrap(),
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
        assert_eq!(spec_findings(&error,Some(run.source())).unwrap(),json!([{"phase":"validation","condition":"ungrouped_driver_field","requirement":requirement,"spec_paths":[path],"context":context}]).as_array().unwrap().clone());
    }
}

#[test]
fn original_sum_reports_nonnumeric_values_but_publishes_all_missing_groups() {
    use serde_json::{json, Value as Json};
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
        published: Vec<Vec<u8>>,
    }
    impl SourcePort for Host {
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
            serde_json::from_str(include_str!("../fixtures/specs/adam-adlb-ordered-sum.json"))
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
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
        published: Vec<Vec<u8>>,
    }
    impl SourcePort for Host {
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
        serde_json::from_str(include_str!("../fixtures/specs/adam-adlb-ordered-sum.json")).unwrap();
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
        spec_findings(&error, Some(run.source())).unwrap(),
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
        spec_findings(&error, None).unwrap(),
        vec![
            json!({"phase":"validation","condition":"duplicate_derivation","requirement":"REQ-1260","spec_paths":["columns.AVAL.derivation"],"context":{"column":"AVAL","rows":["collected","total"],"not_row_local":"aggregate"}})
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
        spec_findings(&error, None).unwrap(),
        vec![
            json!({"phase":"validation","condition":"undeclared_column","requirement":"REQ-0202","spec_paths":["rows[0].derivations.ABSENT"],"context":{"column":"ABSENT"}}),
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
            serde_json::from_str(include_str!("../fixtures/specs/schema-lookup.json")).unwrap();
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
        serde_json::from_str(include_str!("../fixtures/specs/lookup-failures.json")).unwrap();
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
            serde_json::from_str(include_str!("../fixtures/specs/schema-lookup.json")).unwrap();
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
        let mut expected: Json = serde_json::from_str(include_str!(
            "../fixtures/specs/schema-window-functions.json"
        ))
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
        serde_json::from_str(include_str!("../fixtures/specs/window-failures.json")).unwrap();
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
        let mut expected: Json = serde_json::from_str(include_str!(
            "../fixtures/specs/schema-window-functions.json"
        ))
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
    use yamaa_core::{inheritance as graph, inheritance_preparation as lifecycle};
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
            matches!(error,yamaa_core::specification::PrepareError::Invalid(ref items) if items.iter().any(|item|matches!(item, yamaa_core::specification::PreflightFinding::Scalar(diagnostic) if diagnostic.code == yamaa_core::diagnostic::ConditionCode::SubmissionReservedMetadata && diagnostic.spec_paths == [path])))
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
        fn capture_observed(
            &mut self,
            source: &yamaa_core::specification::SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(
            &mut self,
            _source: &yamaa_core::specification::SourceDeclaration,
        ) -> Result<(), Self::Error> {
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
            serde_json::from_str(include_str!("../fixtures/specs/schema-inheritance.json"))
                .unwrap();
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
