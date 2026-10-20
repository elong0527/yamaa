use crate::support::producer as support;
use std::collections::BTreeSet;
use support::{consumer, document, link, links, producer, Tree};
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    producer_contract,
    specification::{PrepareError, PreparedSpecification},
};
fn first(result: Result<PreparedSpecification, PrepareError>) -> Diagnostic {
    let Err(PrepareError::Invalid(findings)) = result else {
        panic!("link admission findings")
    };
    findings.into_iter().next().unwrap().diagnostic()
}
pub fn reached() -> BTreeSet<ConditionCode> {
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
    reached.insert(check(
        first(PreparedSpecification::prepare_linked(
            &consumer(Some(Tree::map(vec![]))),
            None,
            links(&[link(&source)]),
        )),
        "redundant_field_type",
        "INPUT-6",
        &["input.SRC.types"],
    ));
    let invalid = document(
        Tree::map(vec![(
            "RAW",
            Tree::map(vec![("path", Tree::text("raw.csv"))]),
        )]),
        &[("ID", "int", None)],
        &["ID"],
        false,
    );
    reached.insert(check(
        first(PreparedSpecification::prepare_linked(
            &consumer(None),
            None,
            links(&[link(&invalid)]),
        )),
        "invalid_producer_contract",
        "INPUT-10",
        &["input.SRC.schema.columns.ID.label"],
    ));
    let contract = producer_contract::prepare(&source).unwrap();
    reached.insert(check(
        contract
            .validate_header("SRC", &["VALUE", "ID"])
            .unwrap()
            .unwrap(),
        "producer_contract_mismatch",
        "INPUT-11",
        &["input.SRC.schema", "input.SRC.path"],
    ));
    reached
}
