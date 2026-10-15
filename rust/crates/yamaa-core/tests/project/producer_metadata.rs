use crate::support::producer as support;
use support::{candidate, consumer, producer};
use yamaa_core::producer_admission;
use yamaa_core::{producer_admission::Error, specification::PrepareError};
#[test]
fn checked_metadata_retains_producer_identity_and_ordinary_domain_refusal() {
    let source = producer();
    let consumer = consumer(None);
    let checked =
        producer_admission::prepare("/consumer.yaml", &consumer, &[candidate(&source)], None)
            .unwrap();
    assert_eq!(checked.producers()[0].producer_identity(), "/producer.yaml");
    assert!(matches!(
        yamaa_core::domain::check(&consumer),
        Err(PrepareError::Unsupported(_))
    ));
}
#[test]
fn incomplete_metadata_fails_without_a_source_decoder_or_runtime_callback_port() {
    let consumer = consumer(None);
    assert!(
        matches!(producer_admission::prepare("/consumer.yaml",&consumer,&[],None),Err(Error::Invalid(findings)) if findings[0].spec_paths==["input.SRC.schema"])
    );
}
