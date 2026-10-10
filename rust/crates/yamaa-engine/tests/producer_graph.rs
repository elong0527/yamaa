#[allow(dead_code)]
#[path = "../../yamaa-core/tests/support/producer_graph.rs"]
mod support;
use support::{candidate, document, environment, node};
use yamaa_core::{
    producer_graph::{Error, Limits},
    specification::PrepareError,
};
use yamaa_engine::producer_graph;

#[test]
fn complete_graph_metadata_never_grants_the_ordinary_build_capability() {
    let root = document(
        &[("FIRST", Some("/p")), ("SECOND", Some("/p"))],
        Some("alpha"),
    );
    let p = document(&[("RAW", None)], Some("beta"));
    let links = [
        candidate("/root", "SECOND", "/p", &p),
        candidate("/root", "FIRST", "/p", &p),
    ];
    let checked = producer_graph::check(
        "/root",
        &[node("/p", &p, &[]), node("/root", &root, &links)],
        environment(&["beta", "unused", "alpha"]),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(checked.metadata().order(), [0, 1]);
    assert_eq!(checked.metadata().root(), 1);
    assert_eq!(checked.metadata().called_functions(), [0, 2]);
    assert_eq!(checked.metadata().nodes()[1].activation_slots(), [1]);
    assert!(
        matches!(checked.execution_capability(), Err(PrepareError::Unsupported(features)) if features[0].operation == "producer_workflow" && features[0].path == "input.schema")
    );
    assert!(matches!(
        yamaa_engine::domain::check(&root),
        Err(PrepareError::Unsupported(_))
    ));
}

#[test]
fn incomplete_graph_refuses_without_accepting_source_runtime_or_activation_ports() {
    let root = document(&[("P", Some("/p"))], None);
    let p = document(&[("RAW", None)], None);
    let links = [candidate("/root", "P", "/p", &p)];
    assert!(matches!(
        producer_graph::check(
            "/root",
            &[node("/root", &root, &links)],
            environment(&[]),
            Limits::default()
        ),
        Err(Error::Boundary {
            reason: "missing_producer_node",
            ..
        })
    ));
}
