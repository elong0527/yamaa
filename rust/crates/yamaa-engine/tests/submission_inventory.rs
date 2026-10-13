use std::rc::Rc;
use yamaa_core::{
    schema::{Document, DocumentNode as N},
    submission_inventory::Inventory,
};
use yamaa_engine::submission_inventory::{self as service, Ports};
fn domain(name: &str) -> Document {
    Document::new(
        vec![
            N::Text("domain".into()),
            N::Text(name.into()),
            N::Mapping(vec![(0, 1)]),
        ],
        2,
        Default::default(),
    )
    .unwrap()
}
fn environment() -> Document {
    Document::new(
        vec![
            N::Text("sdtm".into()),
            N::Text("specs".into()),
            N::Text("first.yaml".into()),
            N::Text("second.yaml".into()),
            N::Sequence(vec![2, 3]),
            N::Mapping(vec![(1, 4)]),
            N::Mapping(vec![(0, 5)]),
        ],
        6,
        Default::default(),
    )
    .unwrap()
}
struct Captured {
    document: Document,
    identity: String,
}
#[derive(Debug)]
struct Original(Rc<()>);
struct Metadata {
    calls: Vec<(String, String)>,
    fail: bool,
    original: Rc<()>,
}
impl Ports for Metadata {
    type Specification = Captured;
    type Error = Original;
    fn capture(&mut self, declaring: &str, written: &str) -> Result<Captured, Original> {
        self.calls.push((declaring.into(), written.into()));
        if self.fail && self.calls.len() == 2 {
            return Err(Original(Rc::clone(&self.original)));
        }
        Ok(Captured {
            document: domain(if self.calls.len() == 1 { "DM" } else { "ADSL" }),
            identity: format!("{declaring}/{written}"),
        })
    }
    fn document(captured: &Captured) -> &Document {
        &captured.document
    }
    fn identity(captured: &Captured) -> &str {
        &captured.identity
    }
}
#[test]
fn native_capture_retains_original_metadata_owners_and_never_accepts_a_data_port() {
    let mut port = Metadata {
        calls: vec![],
        fail: false,
        original: Rc::new(()),
    };
    let held = service::capture(&environment(), "original/environment.yaml", &mut port).unwrap();
    assert_eq!(held.inventory.names(), ["ADSL", "DM"]);
    assert_eq!(held.specifications.len(), 2);
    assert_eq!(
        port.calls,
        [
            ("original/environment.yaml".into(), "first.yaml".into()),
            ("original/environment.yaml".into(), "second.yaml".into())
        ]
    );
}
#[test]
fn an_uncloneable_boundary_failure_keeps_its_exact_original_payload_and_written_request() {
    let original = Rc::new(());
    let mut port = Metadata {
        calls: vec![],
        fail: true,
        original: Rc::clone(&original),
    };
    let Err(service::Error::Port { written, error }) =
        service::capture(&environment(), "environment.yaml", &mut port)
    else {
        panic!("original capture failure")
    };
    assert_eq!(written, "second.yaml");
    assert!(Rc::ptr_eq(&original, &error.0));
    assert_eq!(port.calls.len(), 2);
}
#[test]
fn inventory_limits_refuse_before_copying_a_partial_dataset_vocabulary() {
    let input = domain("DM");
    let error = Inventory::from_specifications(
        &[&input],
        yamaa_core::project_limits::Limits {
            work: 1,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        yamaa_core::submission_inventory::Error::Limit(yamaa_core::project_limits::Limit {
            resource: yamaa_core::project_limits::Resource::Work
        })
    );
    assert!(Inventory::default().names().is_empty());
}
