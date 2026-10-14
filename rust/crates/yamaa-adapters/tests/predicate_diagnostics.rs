//! Complete original-document outcomes captured independently from Python.
use std::sync::Arc;
use yamaa_adapters::{
    specification_report::{self, ArtifactPort, Identity},
    specification_run::{PreparedRun, SourcePort},
    specification_source::Source,
};
use yamaa_core::specification::SourceDeclaration;

struct Port {
    captures: usize,
}
impl SourcePort for Port {
    type Error = ();
    fn capture_reads(&self) -> usize {
        self.captures
    }
    fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
        assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
        assert!(maximum >= 9);
        self.captures += 1;
        Ok(Arc::from(b"ID,V\n1,2\n".as_slice()))
    }
}
impl ArtifactPort for Port {
    type Error = ();
    fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
        panic!("failed predicate attempt cannot publish")
    }
}
#[test]
fn original_regex_and_temporal_reports_match_independently_captured_reference() {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in include_str!("fixtures/predicate_context_diagnostics.tsv")
        .lines()
        .skip(1)
    {
        let fields = row.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 3);
        let prepared = schema
            .prepare_standalone(Source {
                identity: "spec.yaml".into(),
                bytes: fields[1].as_bytes().to_vec(),
            })
            .unwrap();
        let run = PreparedRun::prepare(prepared).unwrap();
        let mut port = Port { captures: 0 };
        let attempt = run.execute_with_port(&mut port);
        let example = format!("predicate-context-{}", fields[0]);
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
        let expected: serde_json::Value = serde_json::from_str(fields[2]).unwrap();
        assert_eq!(result.observations(), expected, "{}", fields[0]);
        assert!(result.output().is_none());
        drop(attempt);
        drop(run);
        for _ in 0..2 {
            assert!(matches!(
                result.save(&mut port),
                Err(yamaa_engine::specification_output::SaveError::FailedBuild)
            ));
            assert_eq!(result.observations(), expected);
        }
        assert_eq!(port.captures, 1);
        count += 1;
    }
    assert_eq!(count, 2);
}
