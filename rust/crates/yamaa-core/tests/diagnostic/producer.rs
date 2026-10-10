#[allow(dead_code)]
#[path = "../support/producer.rs"]
mod support;
use std::collections::BTreeSet;
use support::{candidate, consumer, document, producer, Tree};
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    producer_admission::{prepare, Error, Limits},
    producer_contract,
};
pub fn reached() -> BTreeSet<ConditionCode> {
    use Tree::*;
    let mut reached = BTreeSet::new();
    let check = |finding: Diagnostic, condition, requirement, paths: &[&str]| {
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(finding.spec_paths, paths);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        finding.code
    };
    let source = producer();
    let Err(Error::Invalid(findings)) = prepare(
        "/consumer.yaml",
        &consumer(Some(Map(vec![]))),
        &[candidate(&source)],
        None,
        Limits::default(),
    ) else {
        panic!("redundant authority")
    };
    reached.insert(check(
        findings.into_iter().next().unwrap(),
        "redundant_field_type",
        "REQ-0523",
        &["input.SRC.types"],
    ));
    let invalid = document(
        Map(vec![("RAW", Map(vec![("path", Text("raw.csv"))]))]),
        &[("ID", "int", None)],
        &["ID"],
        false,
    );
    let Err(Error::Invalid(findings)) = prepare(
        "/consumer.yaml",
        &consumer(None),
        &[candidate(&invalid)],
        None,
        Limits::default(),
    ) else {
        panic!("invalid producer")
    };
    reached.insert(check(
        findings.into_iter().next().unwrap(),
        "invalid_producer_contract",
        "REQ-0534",
        &["input.SRC.schema.columns.ID.label"],
    ));
    let mut mismatch = candidate(&source);
    mismatch.output_identity = "/other.csv";
    let Err(Error::Invalid(findings)) = prepare(
        "/consumer.yaml",
        &consumer(None),
        &[mismatch],
        None,
        Limits::default(),
    ) else {
        panic!("path mismatch")
    };
    reached.insert(check(
        findings.into_iter().next().unwrap(),
        "producer_output_path_mismatch",
        "REQ-0534",
        &["input.SRC.path", "input.SRC.schema"],
    ));
    let contract = producer_contract::prepare(&source, Default::default()).unwrap();
    reached.insert(check(
        contract
            .validate_header("SRC", &["VALUE", "ID"], Default::default())
            .unwrap()
            .unwrap(),
        "producer_contract_mismatch",
        "REQ-0535",
        &["input.SRC.schema", "input.SRC.path"],
    ));
    reached
}
