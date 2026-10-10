use std::{convert::Infallible, sync::Arc};
use yamaa_adapters::{
    specification_report::{self, Error, Identity},
    specification_run::PreparedRun,
    specification_source::Source,
};
use yamaa_core::{
    schema::DocumentNode,
    specification::{CompilationLimits, PrepareError, PreparedSpecification, SourceDeclaration},
};
use yamaa_engine::{specification_output::SaveError, specification_run::SourcePort};

const SPEC: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: output.csv, columns: [ID, VALUE, DOUBLE], decimals: PRECISION}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n  - {name: DOUBLE, type: float, derivation: {compute: {expr: 'VALUE + VALUE'}}}\n";
fn prepared(precision: &str, path: &str, expression: &str) -> PreparedRun {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let bytes = SPEC
        .replace("PRECISION", precision)
        .replace("output.csv", path)
        .replace("VALUE + VALUE", expression);
    let document = schema
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: bytes.into_bytes(),
        })
        .unwrap();
    PreparedRun::prepare(document).unwrap()
}
struct Study(usize);
impl SourcePort for Study {
    type Error = Infallible;
    fn capture_reads(&self) -> usize {
        self.0
    }
    fn capture(&mut self, source: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, Infallible> {
        assert_eq!(source.path, "input.csv");
        self.0 += 1;
        Ok(Arc::from(
            b"ID,VALUE\n1,0.125\n2,1.234\n3,2.345\n".as_slice(),
        ))
    }
}
fn identity() -> Identity<'static> {
    Identity {
        runtime: "python",
        runtime_version: "fixture",
        engine_version: "fixture",
        example: "csv-precision",
        specification: "spec.yaml",
        base_directory: ".",
    }
}
#[derive(Default)]
struct Publisher {
    bytes: Vec<u8>,
    calls: usize,
    reject: bool,
}
impl specification_report::ArtifactPort for Publisher {
    type Error = &'static str;
    fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        self.calls += 1;
        assert_eq!(path, "output.csv");
        if self.reject {
            return Err("original publication failure");
        }
        self.bytes = content.into();
        Ok(())
    }
}

#[test]
fn whole_result_keeps_unrounded_derivation_and_prepares_exact_saved_bytes_once() {
    let run = prepared("2", "output.csv", "VALUE + VALUE");
    let mut study = Study(0);
    let attempt = run.execute_with_port(&mut study);
    let table = attempt.result.as_ref().unwrap().table.as_ref().unwrap();
    let snapshot: serde_json::Value =
        serde_json::from_str(&yamaa_adapters::table_transport::table_snapshot(table).unwrap())
            .unwrap();
    assert_eq!(
        snapshot["rows"][1][1],
        serde_json::json!({"float": "3ff3be76c8b43958"})
    );
    assert_eq!(
        snapshot["rows"][1][2],
        serde_json::json!({"float": "4003be76c8b43958"})
    );
    let result = specification_report::build_result(&run, &attempt, identity()).unwrap();
    assert_eq!(result.observations()["artifacts"], serde_json::json!([]));
    let mut publisher = Publisher {
        reject: true,
        ..Default::default()
    };
    assert!(matches!(
        result.save(&mut publisher),
        Err(SaveError::Publish("original publication failure"))
    ));
    assert!(publisher.bytes.is_empty());
    publisher.reject = false;
    drop(attempt);
    drop(run);
    let report = result.save(&mut publisher).unwrap();
    let expected = b"ID,VALUE,DOUBLE\n1,0.13,0.25\n2,1.23,2.47\n3,2.35,4.69\n";
    assert_eq!(publisher.bytes, expected);
    assert_eq!(
        report["artifacts"][0]["content"]
            .as_str()
            .unwrap()
            .as_bytes(),
        expected
    );
    result.save(&mut publisher).unwrap();
    assert_eq!(publisher.calls, 3);
    assert_eq!(study.0, 1);
}

#[test]
fn output_precision_rejection_follows_original_execution_failure_and_denies_publication() {
    for (precision, path, expression, condition) in [
        ("-1", "output.csv", "VALUE + VALUE", "invalid_field_type"),
        (
            "2",
            "output.parquet",
            "VALUE + VALUE",
            "decimals_not_applicable",
        ),
        ("-1", "output.csv", "1 / 0", "division_by_zero"),
    ] {
        let run = prepared(precision, path, expression);
        let attempt = run.execute_with_port(&mut Study(0));
        let result = specification_report::build_result(&run, &attempt, identity()).unwrap();
        let report = result.observations();
        assert!(result.output().is_none());
        assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
        assert_eq!(report["diagnostics"][0]["condition"], condition);
        let mut publisher = Publisher::default();
        assert!(matches!(
            result.save(&mut publisher),
            Err(SaveError::FailedBuild)
        ));
        assert_eq!(publisher.calls, 0);
    }
    let run = prepared(
        "999999999999999999999999999999",
        "output.csv",
        "VALUE + VALUE",
    );
    let attempt = run.execute_with_port(&mut Study(0));
    assert!(matches!(
        specification_report::build_result(&run, &attempt, identity()),
        Err(Error::OutputLimit)
    ));
}

#[test]
fn compiler_charges_arbitrary_width_precision_before_cloning_its_declaration() {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let precision = "9".repeat(1000);
    let document = schema
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: SPEC.replace("PRECISION", &precision).into_bytes(),
        })
        .unwrap();
    let text_only = document
        .model()
        .document()
        .nodes()
        .iter()
        .map(|n| match n {
            DocumentNode::Text(s) => s.len(),
            _ => 0,
        })
        .sum();
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            document.model(),
            CompilationLimits {
                model_text_bytes: text_only,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("model_text_bytes"))
    ));
    let compiled = PreparedSpecification::prepare(document.model()).unwrap();
    assert_eq!(compiled.output_decimals(), Some(precision.as_str()));
}
