//! Exercise the shared opt-in fake ports from a downstream integration boundary.
#![cfg(feature = "test-support")]
extern crate alloc;
use yamaa_engine::{
    dataset, function_invocation, project_preparation as metadata, specification_output as output,
    specification_run as run, test_support::*,
};
#[path = "../src/test_support/tests.rs"]
mod tests;

#[derive(Debug)]
struct MetadataOpaque(Box<str>);

#[test]
fn shared_metadata_capture_fake_consumes_owned_outcomes_once_and_logs_before_failure() {
    use metadata::CapturePort;
    let log = EventLog::default();
    let mut port = ScriptedMetadata::new(log.clone());
    port.interrupt_if = |e: &MetadataOpaque| e.0.as_ref() == "interrupt";
    let bytes = b"held".to_vec();
    let snapshot = bytes.as_ptr();
    let error = MetadataOpaque("interrupt".into());
    let address = error.0.as_ptr();
    for outcome in [
        Ok(metadata::Reply::Document(metadata::Source {
            identity: "held.yaml".into(),
            bytes,
        })),
        Err(error),
    ] {
        port.steps.push_back(MetadataCaptureStep {
            kind: metadata::Kind::Function,
            declaring_source: "environment.yaml".into(),
            written: "function.yaml".into(),
            remaining_bytes: 42,
            remaining_identity_bytes: 23,
            outcome,
        });
    }
    let request = || metadata::Request {
        kind: metadata::Kind::Function,
        declaring_source: "environment.yaml",
        written: "function.yaml",
        remaining_bytes: 42,
        remaining_identity_bytes: 23,
    };
    let metadata::Reply::Document(source) = port.capture(request()).unwrap() else {
        panic!()
    };
    assert_eq!(source.bytes.as_ptr(), snapshot);
    let error = port.capture(request()).unwrap_err();
    assert_eq!(error.0.as_ptr(), address);
    assert!(port.is_interrupt(&error));
    assert!(port.steps.is_empty());
    assert_eq!(log.events().len(), 2);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| port.capture(request()))).is_err()
    );
    assert_eq!(log.events().len(), 3);
}

#[test]
fn shared_metadata_decoder_fake_retains_source_and_document_and_moves_nonclone_errors() {
    use metadata::DecodePort;
    use yamaa_core::schema::{Document, DocumentLimits, DocumentNode};
    let log = EventLog::default();
    let mut decoder = ScriptedMetadataDecoder::new(log.clone());
    let source = metadata::Source {
        identity: "environment.yaml".into(),
        bytes: b"{}".to_vec(),
    };
    let bytes = source.bytes.as_ptr();
    let document = Document::new(
        vec![DocumentNode::Mapping(vec![])],
        0,
        DocumentLimits::default(),
    )
    .unwrap();
    let nodes = document.nodes().as_ptr();
    decoder.steps.push_back(MetadataDecodeStep {
        source: source.clone(),
        target: metadata::DecodeTarget::Environment,
        outcome: Ok(document),
    });
    let retained = decoder
        .decode(source, metadata::DecodeTarget::Environment)
        .unwrap();
    assert_eq!(retained.source.bytes.as_ptr(), bytes);
    assert_eq!(retained.document.nodes().as_ptr(), nodes);
    let error = MetadataOpaque("decode refused".into());
    let address = error.0.as_ptr();
    decoder.steps.push_back(MetadataDecodeStep {
        source: retained.source.clone(),
        target: metadata::DecodeTarget::Function,
        outcome: Err(error),
    });
    let error = decoder
        .decode(retained.source, metadata::DecodeTarget::Function)
        .unwrap_err();
    assert_eq!(error.0.as_ptr(), address);
    assert!(decoder.steps.is_empty());
    assert_eq!(log.events().len(), 2);
}
