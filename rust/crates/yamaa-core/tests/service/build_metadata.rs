//! Independent original-entry and admission-order controls for the shared Build boundary.
use std::rc::Rc;
use yamaa_core::build::{self, MetadataAdmissionPort, SelectionFailure};
use yamaa_core::schema::{Document, DocumentNode as N};

struct Original(Rc<usize>);
#[derive(Default)]
struct Port {
    events: Vec<&'static str>,
    changed: Option<Original>,
}
impl MetadataAdmissionPort for Port {
    type Error = Original;

    fn verify_metadata(&mut self) -> Result<(), (String, Original)> {
        self.events.push("verify original snapshots");
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
    )
    .unwrap()
}
fn select(
    document: &Document,
    port: &mut Port,
) -> Result<build::EntrySelection, SelectionFailure<Original>> {
    build::select_entry(document, port)
}

#[test]
fn original_schema_declarations_select_producers_without_defaults_or_study_authority() {
    for producer in [false, true] {
        let mut port = Port::default();
        let input = document(producer);
        let selected = match select(&input, &mut port) {
            Ok(selected) => selected,
            Err(_) => panic!("admitted original entry"),
        };
        assert_eq!(selected.is_producer(), producer);
        assert_eq!(port.events, ["verify original snapshots"]);
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
    )
    .unwrap();
    let mut port = Port::default();
    assert!(matches!(select(&input, &mut port), Ok(selected) if !selected.is_producer()));
    assert_eq!(port.events, ["verify original snapshots"]);
}

#[test]
fn snapshot_refusal_never_returns_a_selection_and_moves_the_original_error() {
    let original = Rc::new(23);
    let mut port = Port {
        changed: Some(Original(Rc::clone(&original))),
        ..Default::default()
    };
    match select(&document(true), &mut port) {
        Err(SelectionFailure::Snapshot { identity, error }) => {
            assert_eq!(identity, "original/spec.yaml");
            assert!(Rc::ptr_eq(&original, &error.0));
        }
        _ => panic!("retained changed snapshot"),
    }
    assert_eq!(port.events, ["verify original snapshots"]);
}
