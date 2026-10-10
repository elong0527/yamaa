#[allow(dead_code)]
#[path = "../../yamaa-core/tests/support/producer.rs"]
mod support;
use support::{candidate, consumer, producer};
use yamaa_core::{
    producer_admission::{Error, Limits},
    specification::PrepareError,
};
use yamaa_engine::producer_admission;
#[test]
fn checked_metadata_never_grants_the_ordinary_build_or_activation_capability() {
    let source = producer();
    let consumer = consumer(None);
    let checked = producer_admission::check(
        "/consumer.yaml",
        &consumer,
        &[candidate(&source)],
        None,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        checked.metadata().producers()[0].producer_identity(),
        "/producer.yaml"
    );
    let Err(PrepareError::Unsupported(features)) = checked.execution_capability() else {
        panic!("producer workflow is not qualified")
    };
    assert_eq!(features[0].operation, "producer_workflow");
    assert_eq!(features[0].path, "input.SRC.schema");
    assert!(matches!(
        yamaa_engine::domain::check(&consumer),
        Err(PrepareError::Unsupported(_))
    ));
}
#[test]
fn incomplete_metadata_fails_without_a_source_decoder_or_runtime_callback_port() {
    let consumer = consumer(None);
    assert!(
        matches!(producer_admission::check("/consumer.yaml",&consumer,&[],None,Limits::default()),Err(Error::Invalid(findings)) if findings[0].spec_paths==["input.SRC.schema"])
    );
}
