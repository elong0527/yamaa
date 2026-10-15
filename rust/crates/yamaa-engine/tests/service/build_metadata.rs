//! Independent original-entry and admission-order controls for the shared Build boundary.
use std::rc::Rc;
use yamaa_core::schema::{Document, DocumentNode as N};
use yamaa_engine::build::{self, EntryView, MetadataAdmissionPort, SelectionFailure};

struct Original(Rc<usize>);
#[derive(Default)]
struct Port {
    events: Vec<(&'static str, usize)>,
    quota: Option<Original>,
    changed: Option<Original>,
}
impl MetadataAdmissionPort for Port {
    type Error = Original;
    fn charge_work(&mut self, work: usize) -> Result<(), Original> {
        self.events.push(("admit original arena", work));
        self.quota.take().map_or(Ok(()), Err)
    }
    fn verify_metadata(&mut self) -> Result<(), (String, Original)> {
        self.events.push(("verify original snapshots", 0));
        self.changed
            .take()
            .map_or(Ok(()), |error| Err(("original/spec.yaml".into(), error)))
    }
}
fn document(producer: bool) -> Document {
    Document::new(
        vec![
            N::Text("schema".into()),
            if producer {
                N::Text("producer.yaml".into())
            } else {
                N::Null
            },
            N::Mapping(vec![(0, 1)]),
            N::Text("FIRST".into()),
            N::Text("schema".into()),
            N::Null,
            N::Mapping(vec![(4, 5)]),
            N::Text("SECOND".into()),
            N::Mapping(vec![(3, 2), (7, 6)]),
            N::Text("input".into()),
            N::Mapping(vec![(9, 8)]),
        ],
        10,
        Default::default(),
    )
    .unwrap()
}
fn select(
    document: &Document,
    model_nodes: usize,
    port: &mut Port,
) -> Result<build::EntrySelection, SelectionFailure<Original>> {
    build::select_entry(
        EntryView {
            original: document,
            model_nodes,
        },
        16,
        port,
    )
}

#[test]
fn original_schema_declarations_select_producers_without_defaults_or_study_authority() {
    for producer in [false, true] {
        let mut port = Port::default();
        let input = document(producer);
        let selected = match select(&input, 16, &mut port) {
            Ok(selected) => selected,
            Err(_) => panic!("admitted original entry"),
        };
        assert_eq!(selected.is_producer(), producer);
        assert_eq!(
            port.events,
            [
                ("admit original arena", 11),
                ("verify original snapshots", 0)
            ]
        );
    }
}

#[test]
fn root_metadata_named_schema_does_not_select_a_producer() {
    let input = Document::new(
        vec![
            N::Text("schema".into()),
            N::Text("producer.yaml".into()),
            N::Mapping(vec![(0, 1)]),
        ],
        2,
        Default::default(),
    )
    .unwrap();
    let mut port = Port::default();
    assert!(matches!(select(&input, 3, &mut port), Ok(selected) if !selected.is_producer()));
    assert_eq!(
        port.events,
        [
            ("admit original arena", 3),
            ("verify original snapshots", 0)
        ]
    );
}

#[test]
fn original_arena_quota_precedes_model_capacity_and_preserves_the_original_payload() {
    let original = Rc::new(17);
    let mut port = Port {
        quota: Some(Original(Rc::clone(&original))),
        ..Default::default()
    };
    match select(&document(true), 17, &mut port) {
        Err(SelectionFailure::Resource(error)) => assert!(Rc::ptr_eq(&original, &error.0)),
        _ => panic!("original quota refusal must precede model limit"),
    }
    assert_eq!(port.events, [("admit original arena", 11)]);
}

#[test]
fn model_capacity_refusal_stops_before_snapshot_verification() {
    let mut port = Port::default();
    assert!(matches!(
        select(&document(true), 17, &mut port),
        Err(SelectionFailure::Limit("producer_graph_document_nodes"))
    ));
    assert_eq!(port.events, [("admit original arena", 11)]);
}

#[test]
fn snapshot_refusal_never_returns_a_selection_and_moves_the_original_error() {
    let original = Rc::new(23);
    let mut port = Port {
        changed: Some(Original(Rc::clone(&original))),
        ..Default::default()
    };
    match select(&document(true), 16, &mut port) {
        Err(SelectionFailure::Snapshot { identity, error }) => {
            assert_eq!(identity, "original/spec.yaml");
            assert!(Rc::ptr_eq(&original, &error.0));
        }
        _ => panic!("retained changed snapshot"),
    }
    assert_eq!(
        port.events,
        [
            ("admit original arena", 11),
            ("verify original snapshots", 0)
        ]
    );
}
