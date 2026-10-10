#[path = "support/producer_graph.rs"]
mod support;
use support::{candidate, document, environment, node};
use yamaa_core::{
    producer_graph::{prepare, Error, Limits},
    specification::PrepareError,
};

#[test]
fn diamond_and_shared_aliases_retain_authored_order_but_compile_each_identity_once() {
    let root = document(
        &[
            ("RIGHT", Some("/right")),
            ("LEFT", Some("/left")),
            ("AGAIN", Some("/left")),
        ],
        None,
    );
    let left = document(&[("LEAF", Some("/leaf"))], None);
    let right = document(&[("LEAF", Some("/leaf"))], None);
    let leaf = document(&[("RAW", None)], None);
    let links = [
        candidate("/root", "AGAIN", "/left", &left),
        candidate("/root", "LEFT", "/left", &left),
        candidate("/root", "RIGHT", "/right", &right),
    ];
    let l = [candidate("/left", "LEAF", "/leaf", &leaf)];
    let r = [candidate("/right", "LEAF", "/leaf", &leaf)];
    let graph = prepare(
        "/root",
        &[
            node("/root", &root, &links),
            node("/left", &left, &l),
            node("/leaf", &leaf, &[]),
            node("/right", &right, &r),
        ],
        environment(&[]),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(graph.root(), 0);
    assert_eq!(graph.order(), [2, 3, 1, 0]);
    assert_eq!(graph.nodes().len(), 4);
    assert_eq!(graph.nodes()[0].dependencies(), [3, 1, 1]);
    assert_eq!(
        graph.nodes()[0]
            .producers()
            .iter()
            .map(|p| p.dataset())
            .collect::<Vec<_>>(),
        ["RIGHT", "LEFT", "AGAIN"]
    );
    assert!(graph.called_functions().is_empty());
    let PrepareError::Unsupported(features) = graph.execution_refusal() else {
        panic!("metadata graph must refuse execution")
    };
    assert_eq!(
        features
            .iter()
            .map(|f| (f.operation.as_str(), f.path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("producer_workflow", "input.RIGHT.schema"),
            ("producer_workflow", "input.LEFT.schema"),
            ("producer_workflow", "input.AGAIN.schema"),
        ]
    );
    assert!(matches!(
        yamaa_core::specification::PreparedSpecification::prepare(&root),
        Err(PrepareError::Unsupported(_))
    ));
}

#[test]
fn local_slots_project_into_unique_graph_selection_in_environment_order() {
    let root = document(&[("PROD", Some("/producer"))], Some("alpha"));
    let producer = document(&[("RAW", None)], Some("beta"));
    let links = [candidate("/root", "PROD", "/producer", &producer)];
    let env = environment(&["unused", "beta", "alpha"]);
    let cases = env.functions()[1].definition().tests.as_ptr();
    let graph = prepare(
        "/root",
        &[
            node("/root", &root, &links),
            node("/producer", &producer, &[]),
        ],
        env,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(graph.order(), [1, 0]);
    assert_eq!(graph.called_functions(), [1, 2]);
    assert_eq!(graph.nodes()[0].called_functions(), [2]);
    assert_eq!(graph.nodes()[0].activation_slots(), [1]);
    assert_eq!(graph.nodes()[1].called_functions(), [1]);
    assert_eq!(graph.nodes()[1].activation_slots(), [0]);
    assert_eq!(
        graph.environment().functions()[1]
            .definition()
            .tests
            .as_ptr(),
        cases
    );
    for n in graph.nodes() {
        for (&local, &slot) in n.called_functions().iter().zip(n.activation_slots()) {
            assert_eq!(local, graph.called_functions()[slot]);
        }
    }
}

#[test]
fn recursive_and_self_cycles_retain_closed_identity_route_without_compiling_calls() {
    use yamaa_core::{diagnostic::ContextValue as V, value::Value};
    let a = document(&[("B", Some("/b"))], Some("unknown"));
    let b = document(&[("C", Some("/c"))], None);
    let c = document(&[("A", Some("/a"))], None);
    let aa = [candidate("/a", "B", "/b", &b)];
    let bb = [candidate("/b", "C", "/c", &c)];
    let cc = [candidate("/c", "A", "/a", &a)];
    let Err(Error::Cycle {
        node: 2,
        diagnostic,
    }) = prepare(
        "/a",
        &[
            node("/a", &a, &aa),
            node("/b", &b, &bb),
            node("/c", &c, &cc),
        ],
        environment(&[]),
        Limits::default(),
    )
    else {
        panic!("cycle before unknown call")
    };
    assert_eq!(
        (
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        ("producer_workflow_cycle", Some("REQ-0534"))
    );
    assert_eq!(diagnostic.spec_paths, ["input.A.schema"]);
    assert_eq!(
        diagnostic.context["cycle"],
        V::Sequence(
            ["/a", "/b", "/c", "/a"]
                .into_iter()
                .map(|s| V::Scalar(Value::Str(s.into())))
                .collect()
        )
    );
    let own = document(&[("SELF", Some("/self"))], None);
    let links = [candidate("/self", "SELF", "/self", &own)];
    assert!(matches!(
        prepare(
            "/self",
            &[node("/self", &own, &links)],
            environment(&[]),
            Limits::default()
        ),
        Err(Error::Cycle { .. })
    ));
}

#[test]
fn closure_rejects_missing_duplicate_unreachable_or_replaced_node() {
    let root = document(&[("P", Some("/p"))], None);
    let p = document(&[("RAW", None)], None);
    let replaced = document(&[("RAW", None)], Some("unknown"));
    let links = [candidate("/root", "P", "/p", &p)];
    for (nodes, reason) in [
        (vec![node("/root", &root, &links)], "missing_producer_node"),
        (
            vec![
                node("/root", &root, &links),
                node("/p", &p, &[]),
                node("/p", &p, &[]),
            ],
            "duplicate_or_empty_identity",
        ),
        (
            vec![
                node("/root", &root, &links),
                node("/p", &p, &[]),
                node("/extra", &p, &[]),
            ],
            "unreachable_node",
        ),
        (
            vec![node("/root", &root, &links), node("/p", &replaced, &[])],
            "contradictory_producer_document",
        ),
    ] {
        assert!(
            matches!(prepare("/root", &nodes, environment(&[]), Limits::default()), Err(Error::Boundary { reason: actual, .. }) if actual == reason)
        );
    }
    assert!(matches!(
        prepare(
            "/missing",
            &[node("/p", &p, &[])],
            environment(&[]),
            Limits::default()
        ),
        Err(Error::Boundary {
            reason: "missing_root",
            ..
        })
    ));
}

#[test]
fn invalid_producer_and_consumer_calls_cannot_be_hidden_by_successful_graph_metadata() {
    for bad_producer in [true, false] {
        let root = document(
            &[("P", Some("/p"))],
            if bad_producer { None } else { Some("unknown") },
        );
        let p = document(
            &[("RAW", None)],
            if bad_producer { Some("unknown") } else { None },
        );
        let links = [candidate("/root", "P", "/p", &p)];
        let Err(Error::Compilation {
            node: index,
            error: PrepareError::Invalid(findings),
        }) = prepare(
            "/root",
            &[node("/root", &root, &links), node("/p", &p, &[])],
            environment(&["id"]),
            Limits::default(),
        )
        else {
            panic!("complete static call checking")
        };
        assert_eq!(index, usize::from(bad_producer));
        assert_eq!(
            findings[0].diagnostic().definition().condition,
            "unknown_project_function"
        );
    }
}

#[test]
fn aggregate_graph_quotas_refuse_before_metadata_capability() {
    let root = document(&[("P", Some("/p"))], Some("id"));
    let p = document(&[("RAW", None)], Some("id"));
    let links = [candidate("/root", "P", "/p", &p)];
    let nodes = [node("/root", &root, &links), node("/p", &p, &[])];
    for (limits, resource) in [
        (
            Limits {
                nodes: 1,
                ..Default::default()
            },
            "producer_graph_nodes",
        ),
        (
            Limits {
                edges: 0,
                ..Default::default()
            },
            "producer_graph_edges",
        ),
        (
            Limits {
                document_nodes: 0,
                ..Default::default()
            },
            "producer_graph_document_nodes",
        ),
        (
            Limits {
                text_bytes: 0,
                ..Default::default()
            },
            "producer_graph_text_bytes",
        ),
        (
            Limits {
                work: 0,
                ..Default::default()
            },
            "producer_graph_work",
        ),
        (
            Limits {
                slots: 1,
                ..Default::default()
            },
            "producer_graph_slots",
        ),
        (
            Limits {
                functions: 0,
                ..Default::default()
            },
            "producer_graph_functions",
        ),
        (
            Limits {
                parameters: 1,
                ..Default::default()
            },
            "producer_graph_parameters",
        ),
    ] {
        assert!(
            matches!(prepare("/root", &nodes, environment(&["id"]), limits), Err(Error::Limit(name)) if name == resource)
        );
    }
}

#[test]
fn shared_function_has_one_graph_slot_and_shared_output_metadata_cannot_diverge() {
    let root = document(
        &[("LEFT", Some("/left")), ("RIGHT", Some("/right"))],
        Some("id"),
    );
    let left = document(&[("LEAF", Some("/leaf"))], Some("id"));
    let right = document(&[("LEAF", Some("/leaf"))], Some("id"));
    let leaf = document(&[("RAW", None)], Some("id"));
    let links = [
        candidate("/root", "LEFT", "/left", &left),
        candidate("/root", "RIGHT", "/right", &right),
    ];
    let l = [candidate("/left", "LEAF", "/leaf", &leaf)];
    let mut r = [candidate("/right", "LEAF", "/leaf", &leaf)];
    let graph = prepare(
        "/root",
        &[
            node("/root", &root, &links),
            node("/left", &left, &l),
            node("/right", &right, &r),
            node("/leaf", &leaf, &[]),
        ],
        environment(&["unused", "id"]),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(graph.called_functions(), [1]);
    for node in graph.nodes() {
        assert_eq!(node.activation_slots(), [0]);
    }
    r[0].source_identity = "/different.csv";
    r[0].output_identity = "/different.csv";
    assert!(matches!(
        prepare(
            "/root",
            &[
                node("/root", &root, &links),
                node("/left", &left, &l),
                node("/right", &right, &r),
                node("/leaf", &leaf, &[])
            ],
            environment(&["id"]),
            Limits::default()
        ),
        Err(Error::Boundary {
            reason: "contradictory_producer_metadata",
            ..
        })
    ));
}

#[test]
fn callable_metadata_text_is_charged_even_when_absent_from_specification_trees() {
    use yamaa_core::{
        project_environment::{Draft, Environment},
        project_function::Language,
    };
    let mut definition = environment(&["id"]).functions()[0].definition().clone();
    definition.function = format!("project.{}", "a".repeat(8000));
    let env = Environment::admit(
        Language::Python,
        Draft {
            language: Some(Language::Python),
            lock: environment(&[]).lock().cloned(),
            functions: Some(vec![definition]),
            codelists: vec![],
            has_study: false,
            submissions: vec![],
        },
    )
    .unwrap()
    .into_execution();
    let root = document(&[("P", Some("/p"))], Some("id"));
    let p = document(&[("RAW", None)], None);
    let links = [candidate("/root", "P", "/p", &p)];
    assert!(matches!(
        prepare(
            "/root",
            &[node("/root", &root, &links), node("/p", &p, &[])],
            env,
            Limits {
                text_bytes: 4000,
                ..Default::default()
            }
        ),
        Err(Error::Limit("producer_graph_text_bytes"))
    ));
}
