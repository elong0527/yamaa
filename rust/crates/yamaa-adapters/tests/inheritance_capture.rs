use serde_json::{json, Value};
use std::{collections::BTreeMap, rc::Rc};
use yamaa_adapters::{
    shipped_schema, specification_diagnostics,
    specification_source::{
        CapturedSchema, InheritanceError, InheritancePort, Limits, Source, SourceFailure,
    },
};
use yamaa_engine::{inheritance as graph, inheritance_preparation as lifecycle};
const VALID:&str="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
struct Payload {
    identity: Rc<()>,
    allocation: Box<u32>,
}
struct Host {
    bytes: BTreeMap<String, Vec<u8>>,
    calls: Vec<String>,
    failure: Option<Payload>,
    fail_at: usize,
}
impl InheritancePort for Host {
    type Error = Payload;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Payload>> {
        self.calls.push(format!("resolve:{declaring}:{written}"));
        if self.calls.len() == self.fail_at {
            return Err(graph::SourceError::Raised(self.failure.take().unwrap()));
        }
        Ok(graph::Source {
            identity: written.into(),
            display_path: format!("display/{written}"),
        })
    }
    fn capture(
        &mut self,
        source: &graph::Source,
        maximum: usize,
    ) -> Result<Vec<u8>, graph::SourceError<Payload>> {
        self.calls
            .push(format!("capture:{}:{maximum}", source.identity));
        if self.calls.len() == self.fail_at {
            return Err(graph::SourceError::Raised(self.failure.take().unwrap()));
        }
        Ok(self
            .bytes
            .remove(&source.identity)
            .expect("no duplicate capture"))
    }
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
        maximum: usize,
    ) -> Result<String, Payload> {
        self.calls.push(format!(
            "rebase:{}:{}:{written}:{maximum}",
            layer.identity, entry.identity
        ));
        if self.calls.len() == self.fail_at {
            return Err(self.failure.take().unwrap());
        }
        Ok(written.into())
    }
}
fn new_host(bytes: &[u8]) -> Host {
    Host {
        bytes: [("parent.yaml".into(), bytes.to_vec())]
            .into_iter()
            .collect(),
        calls: vec![],
        failure: None,
        fail_at: usize::MAX,
    }
}
fn source(bytes: Vec<u8>) -> Source {
    Source {
        identity: "entry.yaml".into(),
        bytes,
    }
}
fn outcome(error: InheritanceError<Payload>) -> Value {
    match specification_diagnostics::inheritance_failure(error) {
        Ok(s) => serde_json::from_str(&s).unwrap(),
        Err(_) => panic!("semantic report"),
    }
}
fn schema(limits: Limits) -> std::sync::Arc<CapturedSchema> {
    CapturedSchema::admit(
        shipped_schema::capture().unwrap().sources().to_vec(),
        0,
        limits,
    )
    .unwrap()
}
#[test]
fn unchanged_independent_complete_original_preparation_reports_and_effect_counts() {
    for row in include_str!("fixtures/inheritance_preparation.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let mut host = new_host(fields[2].as_bytes());
        let error = match shipped_schema::prepare(
            source(fields[1].as_bytes().to_vec()),
            "written/entry".into(),
            &mut host,
        ) {
            Err(e) => e,
            Ok(_) => panic!("invalid independent original"),
        };
        assert_eq!(
            outcome(error),
            serde_json::from_str::<Value>(fields[4]).unwrap(),
            "{}",
            fields[0]
        );
        assert_eq!(host.calls[0], "resolve:entry.yaml:parent.yaml");
        assert_eq!(
            host.calls[1],
            format!("capture:parent.yaml:{}", 16_777_216 - fields[1].len())
        );
        assert_eq!(
            host.calls
                .iter()
                .filter(|c| c.starts_with("rebase:"))
                .count(),
            fields[3].parse::<usize>().unwrap()
        );
    }
}
#[test]
fn actual_entry_and_parent_snapshots_are_moved_once_with_raw_positions_and_defaults() {
    let root = format!("{VALID}parents: [parent.yaml, parent.yaml]\n").into_bytes();
    let root_pointer = root.as_ptr();
    let mut host = new_host(b"schema_version: '1.0'\n");
    let parent_pointer = host.bytes["parent.yaml"].as_ptr();
    let prepared = match shipped_schema::prepare(source(root), "written/entry".into(), &mut host) {
        Ok(p) => p,
        Err(_) => panic!("valid inheritance"),
    };
    assert_eq!(prepared.source().bytes.as_ptr(), root_pointer);
    assert_eq!(prepared.parents().len(), 1);
    let parent = &prepared.parents()[0];
    assert_eq!(parent.source().bytes.as_ptr(), parent_pointer);
    assert_eq!(parent.source().bytes, b"schema_version: '1.0'\n");
    assert_eq!(
        parent.raw().locations.len(),
        parent.raw().document.nodes().len()
    );
    assert_eq!(
        prepared.raw().locations.len(),
        prepared.raw().document.nodes().len()
    );
    assert_eq!(
        host.calls
            .iter()
            .filter(|s| s.starts_with("resolve:"))
            .count(),
        2
    );
    assert_eq!(
        host.calls
            .iter()
            .filter(|s| s.starts_with("capture:"))
            .count(),
        1
    );
    let d = prepared.model().document();
    let inputs = d.field(d.root(), "input").unwrap();
    let input = d.field(inputs, "SRC").unwrap();
    let policy = d.field(input, "empty_string").unwrap();
    assert_eq!(
        d.nodes()[policy],
        yamaa_core::schema::DocumentNode::Text("missing".into())
    );
    let inherited = prepared.inheritance().unwrap();
    assert_eq!(inherited.layers()[0].source.identity, "parent.yaml");
    assert_eq!(inherited.layers()[1].source.identity, "entry.yaml");
    assert_eq!(inherited.origins().len(), d.nodes().len());
}
#[test]
fn opaque_canonicalize_capture_and_rebase_errors_preserve_their_actual_allocation() {
    for fail_at in [1, 2, 3] {
        let mut host = new_host(b"schema_version: '1.0'\n");
        let identity = Rc::new(());
        let allocation = Box::new(97);
        let pointer = (&*allocation) as *const u32;
        host.failure = Some(Payload {
            identity: identity.clone(),
            allocation,
        });
        host.fail_at = fail_at;
        let root = format!("{VALID}parents: parent.yaml\n");
        let Err(InheritanceError::Preparation {
            error,
            captured_parents,
            ..
        }) = shipped_schema::prepare(source(root.into_bytes()), "entry.yaml".into(), &mut host)
        else {
            panic!()
        };
        let original = match *error {
            lifecycle::Error::Traversal(graph::Error::Host(SourceFailure::Host(p)))
            | lifecycle::Error::Path(SourceFailure::Host(p)) => p,
            _ => panic!(),
        };
        assert!(Rc::ptr_eq(&original.identity, &identity));
        assert_eq!((&*original.allocation) as *const u32, pointer);
        assert_eq!(host.calls.len(), fail_at);
        assert_eq!(captured_parents.len(), usize::from(fail_at == 3));
    }
}
#[test]
fn raw_parent_overruns_refuse_before_codec_and_keep_existing_capture_category() {
    let root = format!("{VALID}parents: parent.yaml\n");
    let parent = b"schema_version: '1.0'\n";
    // The captured schema closure has a larger byte policy; a small root total
    // is represented by a single larger parent reply under the unchanged policy.
    let mut host = new_host(&vec![b' '; 16_777_216 - root.len() + 1]);
    let error = match shipped_schema::prepare(
        source(root.as_bytes().to_vec()),
        "entry.yaml".into(),
        &mut host,
    ) {
        Err(e) => e,
        Ok(_) => panic!(),
    };
    assert_eq!(
        outcome(error),
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"capture","code":"capture_limit"}})
    );
    assert_eq!(host.calls.len(), 2);
    let mut host = new_host(parent);
    assert!(schema(Limits::default())
        .prepare_document(source(root.into_bytes()), "entry.yaml".into(), &mut host)
        .is_ok());
}
