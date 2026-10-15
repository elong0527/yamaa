use crate::support::producer_graph as support;
use support::{candidate, document, environment, node};
use yamaa_core::producer_graph;
use yamaa_core::{producer_graph::Error, specification::PrepareError};

#[test]
fn complete_graph_metadata_retains_identity_order_and_activation_selection() {
    let root = document(
        &[("FIRST", Some("/p")), ("SECOND", Some("/p"))],
        Some("alpha"),
    );
    let p = document(&[("RAW", None)], Some("beta"));
    let links = [
        candidate("/root", "SECOND", "/p", &p),
        candidate("/root", "FIRST", "/p", &p),
    ];
    let checked = producer_graph::prepare(
        "/root",
        &[node("/p", &p, &[]), node("/root", &root, &links)],
        environment(&["beta", "unused", "alpha"]),
    )
    .unwrap();
    assert_eq!(checked.order(), [0, 1]);
    assert_eq!(checked.root(), 1);
    assert_eq!(checked.called_functions(), [0, 2]);
    assert_eq!(checked.nodes()[1].activation_slots(), [1]);
    let leaf =
        producer_graph::prepare("/p", &[node("/p", &p, &[])], environment(&["beta"])).unwrap();
    assert_eq!(leaf.root(), 0);
    assert_eq!(leaf.order(), [0]);
    let Err(PrepareError::Invalid(findings)) = yamaa_core::domain::check(&root) else {
        panic!("metadata alone does not admit the called project function");
    };
    assert!(
        findings.iter().any(
            |finding| finding.diagnostic().definition().condition == "unknown_project_function"
        ),
        "{findings:?}"
    );
}

#[test]
fn incomplete_graph_refuses_without_accepting_source_runtime_or_activation_ports() {
    let root = document(&[("P", Some("/p"))], None);
    let p = document(&[("RAW", None)], None);
    let links = [candidate("/root", "P", "/p", &p)];
    assert!(matches!(
        producer_graph::prepare("/root", &[node("/root", &root, &links)], environment(&[])),
        Err(Error::Boundary {
            reason: "missing_producer_node",
            ..
        })
    ));
}
