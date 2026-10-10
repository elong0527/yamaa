use std::sync::Arc;
use yamaa_adapters::{
    producer_admission::{prepare, Error, Limits, SuppliedProducer},
    shipped_schema,
    specification_source::{InheritancePort, PreparedDocument, Source},
};
use yamaa_core::{producer_admission::Error as CoreError, specification::PrepareError};
use yamaa_engine::inheritance::{Source as FileIdentity, SourceError};
const PRODUCER:&str="schema_version: '1.0'\ndomain: PROD\nkeys: [ID]\ninput: {RAW: never-read.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: float, label: Reported value, derivation: RAW.VALUE}\noutput: {path: './produced.csv', columns: [ID, VALUE]}\n";
const CONSUMER:&str="schema_version: '1.0'\ndomain: CONS\nkeys: [ID]\ninput: {SRC: {path: './produced.csv', schema: nested/producer.yaml}}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
fn standalone(identity: &str, text: &str) -> Arc<PreparedDocument> {
    Arc::new(
        shipped_schema::capture()
            .unwrap()
            .prepare_standalone(Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            })
            .unwrap(),
    )
}
fn supplied(document: Arc<PreparedDocument>) -> SuppliedProducer {
    SuppliedProducer {
        dataset: "SRC".into(),
        schema_identity: "/producer.yaml".into(),
        source_identity: "/artifacts/produced.csv".into(),
        output_identity: "/artifacts/produced.csv".into(),
        document,
    }
}
#[test]
fn original_documents_and_distinct_metadata_remain_held_without_study_authority() {
    let consumer = standalone("/consumer.yaml", CONSUMER);
    let producer = standalone("/producer.yaml", PRODUCER);
    let admitted = prepare(
        Arc::clone(&consumer),
        vec![supplied(Arc::clone(&producer))],
        None,
        Limits::default(),
    )
    .unwrap();
    drop(consumer);
    drop(producer);
    assert_eq!(admitted.consumer().source().bytes, CONSUMER.as_bytes());
    assert_eq!(
        admitted.producers()[0].document.source().bytes,
        PRODUCER.as_bytes()
    );
    let declaration = &admitted.checked().metadata().producers()[0];
    assert_eq!(
        declaration.schema_origin().written(),
        "nested/producer.yaml"
    );
    assert_eq!(
        declaration.schema_origin().declaring_source(),
        "/consumer.yaml"
    );
    assert_eq!(declaration.output_origin().written(), "./produced.csv");
    assert_eq!(
        declaration
            .contract()
            .fields()
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["ID", "VALUE"]
    );
    assert!(
        matches!(admitted.checked().execution_capability(),Err(PrepareError::Unsupported(features)) if features[0].path=="input.SRC.schema")
    );
}
struct Parent {
    text: Vec<u8>,
    calls: Vec<String>,
}
impl InheritancePort for Parent {
    type Error = ();
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<FileIdentity, SourceError<()>> {
        self.calls
            .push(format!("canonicalize:{declaring}:{written}"));
        assert_eq!(written, "parent.yaml");
        Ok(FileIdentity {
            identity: if declaring == "/producer.yaml" {
                "/producer-parent.yaml"
            } else {
                "/parent.yaml"
            }
            .into(),
            display_path: "parent.yaml".into(),
        })
    }
    fn capture(&mut self, source: &FileIdentity, _: usize) -> Result<Vec<u8>, SourceError<()>> {
        self.calls.push(format!("capture:{}", source.identity));
        Ok(self.text.clone())
    }
    fn rebase(
        &mut self,
        layer: &FileIdentity,
        _: &FileIdentity,
        written: &str,
        _: usize,
    ) -> Result<String, ()> {
        self.calls
            .push(format!("rebase:{}:{written}", layer.identity));
        Ok(if layer.identity.ends_with("parent.yaml") {
            format!("internal/parent/{written}")
        } else {
            written.into()
        })
    }
}
fn inherited(identity: &str, entry: &str, parent: &str) -> (Arc<PreparedDocument>, Parent) {
    let mut port = Parent {
        text: parent.as_bytes().to_vec(),
        calls: vec![],
    };
    let document = shipped_schema::prepare(
        Source {
            identity: identity.into(),
            bytes: entry.as_bytes().to_vec(),
        },
        identity.into(),
        &mut port,
    )
    .unwrap();
    (Arc::new(document), port)
}
#[test]
fn inherited_schema_artifact_and_output_paths_keep_independent_authored_layers() {
    let entry="schema_version: '1.0'\nparents: parent.yaml\ndomain: CONS\nkeys: [ID]\ninput: {SRC: {path: './child.csv'}}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
    let parent =
        "schema_version: '1.0'\ninput: {SRC: {path: parent.csv, schema: '../producer.yaml'}}\n";
    let (consumer, consumer_port) = inherited("/consumer.yaml", entry, parent);
    let producer_entry = PRODUCER
        .replace(
            "schema_version: '1.0'",
            "schema_version: '1.0'\nparents: parent.yaml",
        )
        .replace("output: {path: './produced.csv', columns: [ID, VALUE]}", "");
    let producer_parent =
        "schema_version: '1.0'\noutput: {path: '../stored.csv', columns: [ID, VALUE]}\n";
    let (producer, producer_port) = inherited("/producer.yaml", &producer_entry, producer_parent);
    let before = (consumer_port.calls.clone(), producer_port.calls.clone());
    let admitted = prepare(
        Arc::clone(&consumer),
        vec![supplied(producer)],
        None,
        Limits::default(),
    )
    .unwrap();
    let declaration = &admitted.checked().metadata().producers()[0];
    assert_eq!(
        declaration.schema_path(),
        "internal/parent/../producer.yaml"
    );
    assert_eq!(declaration.schema_origin().written(), "../producer.yaml");
    assert_eq!(
        declaration.schema_origin().declaring_source(),
        "/parent.yaml"
    );
    assert_eq!(declaration.input_origin().written(), "./child.csv");
    assert_eq!(
        declaration.input_origin().declaring_source(),
        "/consumer.yaml"
    );
    assert_eq!(declaration.output_origin().written(), "../stored.csv");
    assert_eq!(
        declaration.output_origin().declaring_source(),
        "/producer-parent.yaml"
    );
    assert_eq!(
        declaration.contract().path(),
        "internal/parent/../stored.csv"
    );
    assert_eq!(consumer.parents()[0].source().bytes, parent.as_bytes());
    assert_eq!(
        admitted.producers()[0].document.parents()[0].source().bytes,
        producer_parent.as_bytes()
    );
    assert_eq!(before, (consumer_port.calls, producer_port.calls));
}
#[test]
fn missing_contradictory_and_aggregate_quota_failures_precede_capability() {
    let consumer = standalone("/consumer.yaml", CONSUMER);
    let producer = standalone("/producer.yaml", PRODUCER);
    assert!(matches!(
        prepare(Arc::clone(&consumer), vec![], None, Limits::default()),
        Err(Error::Admission(CoreError::Invalid(_)))
    ));
    let mut wrong = supplied(Arc::clone(&producer));
    wrong.schema_identity = "/wrong.yaml".into();
    assert!(matches!(
        prepare(Arc::clone(&consumer), vec![wrong], None, Limits::default()),
        Err(Error::Admission(CoreError::Invalid(_)))
    ));
    let total = CONSUMER.len() + PRODUCER.len();
    assert!(prepare(
        Arc::clone(&consumer),
        vec![supplied(Arc::clone(&producer))],
        None,
        Limits {
            captured_bytes: total,
            ..Default::default()
        }
    )
    .is_ok());
    assert!(matches!(
        prepare(
            Arc::clone(&consumer),
            vec![supplied(Arc::clone(&producer))],
            None,
            Limits {
                captured_bytes: total - 1,
                ..Default::default()
            }
        ),
        Err(Error::Limit("producer_captured_bytes"))
    ));
    assert!(matches!(
        prepare(
            consumer,
            vec![supplied(producer)],
            None,
            Limits {
                metadata: yamaa_core::producer_admission::Limits {
                    candidates: 0,
                    ..Default::default()
                },
                ..Default::default()
            }
        ),
        Err(Error::Limit("producer_candidates"))
    ));
}
#[test]
fn ordinary_preparation_remains_unsupported_and_inline_types_are_portable() {
    let consumer = standalone("/consumer.yaml", CONSUMER);
    let redundant = CONSUMER.replace(
        "schema: nested/producer.yaml",
        "schema: nested/producer.yaml, types: {ID: int}",
    );
    let redundant = standalone("/consumer.yaml", &redundant);
    let error = prepare(
        redundant,
        vec![supplied(standalone("/producer.yaml", PRODUCER))],
        None,
        Limits::default(),
    );
    let Err(Error::Admission(CoreError::Invalid(findings))) = error else {
        panic!("redundant authority")
    };
    assert_eq!(findings[0].definition().requirement, Some("REQ-0523"));
    assert_eq!(findings[0].spec_paths, ["input.SRC.types.ID"]);
    assert!(matches!(
        yamaa_engine::domain::check(consumer.model()),
        Err(PrepareError::Unsupported(_))
    ));
}
#[test]
fn producer_must_use_the_same_captured_root_closure_and_schema_comparison_is_bounded() {
    use yamaa_adapters::specification_source::CapturedSchema;
    let consumer = standalone("/consumer.yaml", CONSUMER);
    let shipped = shipped_schema::capture().unwrap();
    let mut sources = shipped.sources().to_vec();
    sources[0]
        .bytes
        .extend_from_slice(b"\n# independent changed closure\n");
    let different = CapturedSchema::admit(sources, 0, Default::default()).unwrap();
    let producer = Arc::new(
        different
            .prepare_standalone(Source {
                identity: "/producer.yaml".into(),
                bytes: PRODUCER.as_bytes().to_vec(),
            })
            .unwrap(),
    );
    let Err(Error::Admission(CoreError::Invalid(findings))) = prepare(
        Arc::clone(&consumer),
        vec![supplied(producer)],
        None,
        Limits::default(),
    ) else {
        panic!("captured root mismatch")
    };
    assert_eq!(findings[0].definition().requirement, Some("REQ-0534"));
    assert_eq!(findings[0].spec_paths, ["input.SRC.schema"]);
    let producer = standalone("/producer.yaml", PRODUCER);
    assert!(matches!(
        prepare(
            Arc::clone(&consumer),
            vec![supplied(Arc::clone(&producer))],
            None,
            Limits {
                schema_bytes: 0,
                ..Default::default()
            }
        ),
        Err(Error::Limit("producer_schema_bytes"))
    ));
    assert!(matches!(
        prepare(
            consumer,
            vec![supplied(producer)],
            None,
            Limits {
                schema_modules: 0,
                ..Default::default()
            }
        ),
        Err(Error::Limit("producer_schema_modules"))
    ));
}
