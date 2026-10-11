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

// Retained-arena oracles replacing the retired Python JSON graph consumers.
// Expected resolved YAML and CSV remain the original committed authored bytes.
use yamaa_adapters::yaml_decode::{decode_yaml, DecodeLimits};
use yamaa_core::schema::{Document, DocumentNode as N, SchemaSource};

fn tree(document: &Document, node: usize) -> Value {
    match &document.nodes()[node] {
        N::Null => Value::Null,
        N::Boolean(value) => (*value).into(),
        N::Integer(value) => serde_json::from_str(value).unwrap(),
        N::Float(value) => json!(value),
        N::Text(value) => value.clone().into(),
        N::Sequence(children) => children
            .iter()
            .map(|&child| tree(document, child))
            .collect(),
        N::Mapping(entries) => entries
            .iter()
            .map(|&(key, value)| {
                let N::Text(key) = &document.nodes()[key] else {
                    panic!("independent fixtures use text field names")
                };
                (key.clone(), tree(document, value))
            })
            .collect::<serde_json::Map<_, _>>()
            .into(),
    }
}
fn value(document: &Document) -> Value {
    tree(document, document.root())
}
fn path(document: &Document, fields: &[&str]) -> usize {
    fields.iter().fold(document.root(), |node, field| {
        document.field(node, field).unwrap()
    })
}
fn sequence(document: &Document, node: usize) -> &[usize] {
    let N::Sequence(items) = &document.nodes()[node] else {
        panic!("expected sequence")
    };
    items
}
fn window() -> Value {
    json!({
        "schema_version":"1.0", "domain":"OUT", "base":"SRC", "keys":["ID"],
        "input":{"SRC":{"path":"input.csv", "types":{"SEQ":"int","VAL":"int"}}},
        "output":{"path":"out.csv", "columns":["ID","PREV"]},
        "windows":{"VISITS":{"group_by":["G"],"order_by":["SEQ"]}},
        "columns":[
            {"name":"ID","type":"str","label":"ID","derivation":"SRC.ID"},
            {"name":"G","type":"str","label":"G","derivation":"SRC.G"},
            {"name":"SEQ","type":"int","label":"SEQ","derivation":"SRC.SEQ"},
            {"name":"VAL","type":"int","label":"VAL","derivation":"SRC.VAL"},
            {"name":"PREV","type":"int","label":"Previous value","derivation":{"row_value":{"source":"VAL","offset":-1,"window":"VISITS"}}}
        ]
    })
}

#[derive(Debug, PartialEq)]
enum Event {
    Resolve(String, String),
    Capture(String, String, usize),
    Rebase(String, String, String, usize),
}
#[derive(Default)]
struct Files {
    events: Vec<Event>,
}
impl InheritancePort for Files {
    type Error = std::io::Error;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
        self.events
            .push(Event::Resolve(declaring.into(), written.into()));
        let candidate = std::path::Path::new(declaring)
            .parent()
            .unwrap()
            .join(written);
        let identity = candidate
            .canonicalize()
            .map_err(|_| graph::SourceError::Unavailable)?;
        Ok(graph::Source {
            identity: identity.to_str().unwrap().into(),
            display_path: candidate.to_str().unwrap().into(),
        })
    }
    fn capture(
        &mut self,
        source: &graph::Source,
        maximum: usize,
    ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
        self.events.push(Event::Capture(
            source.identity.clone(),
            source.display_path.clone(),
            maximum,
        ));
        let bytes = std::fs::read(&source.identity).map_err(graph::SourceError::Raised)?;
        assert!(bytes.len() <= maximum);
        Ok(bytes)
    }
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
        maximum: usize,
    ) -> Result<String, Self::Error> {
        self.events.push(Event::Rebase(
            layer.identity.clone(),
            entry.identity.clone(),
            written.into(),
            maximum,
        ));
        // All three independent benchmark cases have parents in the entry directory.
        assert_eq!(
            std::path::Path::new(&layer.identity).parent(),
            std::path::Path::new(&entry.identity).parent()
        );
        Ok(written.into())
    }
}
struct CaseFiles {
    root: std::path::PathBuf,
}
impl CaseFiles {
    fn new(case: &Value) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-inheritance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        for source in case["sources"].as_array().unwrap() {
            let name = source["name"].as_str().unwrap();
            assert_eq!(std::path::Path::new(name).components().count(), 1);
            std::fs::write(root.join(name), source["text"].as_str().unwrap().as_bytes()).unwrap();
        }
        Self { root }
    }
}
impl Drop for CaseFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn three_complete_committed_resolved_documents_survive_native_preparation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/inheritance_preparation_reference.json"
    ))
    .unwrap();
    assert_eq!(fixture["version"], "1");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 3);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let materialized = CaseFiles::new(case);
        let root = &materialized.root;
        let entry = root.join("spec_study.yaml");
        let source = Source {
            identity: entry.to_str().unwrap().into(),
            bytes: std::fs::read(&entry).unwrap(),
        };
        let entry_pointer = source.bytes.as_ptr();
        let mut files = Files::default();
        let prepared =
            shipped_schema::prepare(source, entry.to_str().unwrap().into(), &mut files).unwrap();
        assert_eq!(prepared.source().bytes.as_ptr(), entry_pointer);
        let expected = decode_yaml(
            case["resolved"].as_str().unwrap().as_bytes(),
            DecodeLimits::default(),
        )
        .unwrap();
        let mut expected = value(&expected.document);
        // The committed resolved documents precede the separately specified
        // REQ-1158 default; exact descriptor/default authority is pinned below.
        for declaration in expected["input"].as_object_mut().unwrap().values_mut() {
            declaration["empty_string"] = "missing".into();
        }
        assert_eq!(value(prepared.model().document()), expected, "{name}");
        assert!(prepared
            .model()
            .document()
            .field(prepared.model().document().root(), "parents")
            .is_none());
        let parent_names: &[&str] = if name == "schema-inherited-output" {
            &["spec_shared.yaml"]
        } else {
            &["spec_organization.yaml", "spec_compound.yaml"]
        };
        let expected_parents: Vec<_> = parent_names
            .iter()
            .map(|name| root.join(name).to_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            prepared
                .parents()
                .iter()
                .map(|parent| &parent.source().identity)
                .collect::<Vec<_>>(),
            expected_parents.iter().collect::<Vec<_>>()
        );
        let mut expected_layers = expected_parents.clone();
        expected_layers.push(entry.to_str().unwrap().into());
        assert_eq!(
            prepared
                .inheritance()
                .unwrap()
                .layers()
                .iter()
                .map(|layer| &layer.source.identity)
                .collect::<Vec<_>>(),
            expected_layers.iter().collect::<Vec<_>>()
        );
        let mut tape = Vec::new();
        let mut remaining = 16_777_216 - prepared.source().bytes.len();
        for (index, parent) in prepared.parents().iter().enumerate() {
            let parent_name = parent_names[index];
            tape.push(Event::Resolve(
                entry.to_str().unwrap().into(),
                parent_name.into(),
            ));
            tape.push(Event::Capture(
                parent.source().identity.clone(),
                parent.source().identity.clone(),
                remaining,
            ));
            let original = case["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|source| source["name"] == parent_name)
                .unwrap()["text"]
                .as_str()
                .unwrap();
            assert_eq!(parent.source().bytes, original.as_bytes());
            assert_eq!(
                parent.raw().locations.len(),
                parent.raw().document.nodes().len()
            );
            remaining -= original.len();
        }
        if name != "schema-inherited-output" {
            tape.push(Event::Resolve(
                root.join("spec_compound.yaml").to_str().unwrap().into(),
                "spec_organization.yaml".into(),
            ));
        }
        let rebases: &[(&str, &str)] = match name {
            "schema-inheritance" => &[
                ("spec_organization.yaml", "input/lb.csv"),
                ("spec_organization.yaml", "input/not-used.csv"),
                ("spec_study.yaml", "adlb.csv"),
            ],
            "schema-column-composition" => &[
                ("spec_organization.yaml", "input/lb.csv"),
                ("spec_study.yaml", "adlb.csv"),
            ],
            "schema-inherited-output" => &[
                ("spec_shared.yaml", "adsl.csv"),
                ("spec_study.yaml", "input/dm.csv"),
            ],
            _ => panic!("closed independent case inventory"),
        };
        remaining = 1_048_576;
        for &(layer, written) in rebases {
            tape.push(Event::Rebase(
                root.join(layer).to_str().unwrap().into(),
                entry.to_str().unwrap().into(),
                written.into(),
                remaining,
            ));
            remaining -= written.len();
        }
        assert_eq!(files.events, tape, "{name}");
        let N::Mapping(inputs) =
            &prepared.model().document().nodes()[path(prepared.model().document(), &["input"])]
        else {
            panic!()
        };
        for (key, _) in inputs {
            let N::Text(dataset) = &prepared.model().document().nodes()[*key] else {
                panic!()
            };
            let written = prepared.written_input_origin(dataset, "path").unwrap();
            assert_eq!(
                written.written,
                format!("input/{}.csv", dataset.to_ascii_lowercase())
            );
            let declaring = if name == "schema-inherited-output" {
                &entry
            } else {
                &root.join("spec_organization.yaml")
            };
            assert_eq!(written.declaring_source, declaring.to_str().unwrap());
        }
        // A held result is independent from the temporary physical fixture.
        // A new attempt must refuse the now-missing parent, never consult an
        // ambient repository/benchmark path or silently reuse an earlier result.
        std::fs::remove_file(root.join(parent_names[0])).unwrap();
        assert_eq!(value(prepared.model().document()), expected);
        let mut missing = Files::default();
        let error = shipped_schema::prepare(
            Source {
                identity: entry.to_str().unwrap().into(),
                bytes: prepared.source().bytes.clone(),
            },
            entry.to_str().unwrap().into(),
            &mut missing,
        )
        .unwrap_err();
        let report = specification_diagnostics::inheritance_failure(error).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&report).unwrap(),
            json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parents"],"context":{"path":parent_names[0],"source":entry.to_str().unwrap()}}]}})
        );
        assert_eq!(
            missing.events,
            [Event::Resolve(
                entry.to_str().unwrap().into(),
                parent_names[0].into()
            )]
        );
    }
}

#[test]
fn complete_window_model_keeps_parent_definition_child_override_and_schema_defaults() {
    let mut parent = window();
    parent["windows"]["UNUSED"] = json!({"group_by":["NOT_A_COLUMN"]});
    let child = json!({"schema_version":"1.0","parents":"parent.yaml","windows":{"VISITS":{"order_by":[{"variable":"SEQ","direction":"desc"}]}}});
    let mut host = new_host(parent.to_string().as_bytes());
    let prepared = shipped_schema::prepare(
        source(child.to_string().into_bytes()),
        "entry.yaml".into(),
        &mut host,
    )
    .unwrap_or_else(|_| panic!("valid independent window"));
    let expected = json!({
        "schema_version":"1.0","domain":"OUT","base":"SRC","keys":["ID"],
        "input":{"SRC":{"path":"input.csv","types":{"SEQ":"int","VAL":"int"},"empty_string":"missing"}},
        "output":{"path":"out.csv","columns":["ID","PREV"]},
        "columns":[
            {"name":"ID","type":"str","label":"ID","derivation":{"value":{"source":{"variable":"SRC.ID"}}}},
            {"name":"SEQ","type":"int","label":"SEQ","derivation":{"value":{"source":{"variable":"SRC.SEQ"}}}},
            {"name":"VAL","type":"int","label":"VAL","derivation":{"value":{"source":{"variable":"SRC.VAL"}}}},
            {"name":"PREV","type":"int","label":"Previous value","derivation":{"value":{"row_value":{"source":"VAL","offset":-1,"window":{"order_by":[{"variable":"SEQ","direction":"desc","nulls":"last"}]}}}}}
        ]
    });
    let document = prepared.model().document();
    assert_eq!(value(document), expected);
    let inherited = prepared.inheritance().unwrap();
    assert_eq!(
        inherited
            .layers()
            .iter()
            .map(|layer| layer.source.identity.as_str())
            .collect::<Vec<_>>(),
        ["parent.yaml", "entry.yaml"]
    );
    assert!(inherited.provenance().iter().any(|origin| origin.path
        == "columns.PREV.derivation.value.row_value.window"
        && origin.layer == 0));
    assert!(inherited
        .provenance()
        .iter()
        .any(|origin| origin.path == "windows.VISITS.order_by" && origin.layer == 1));
    let previous = *sequence(document, path(document, &["columns"]))
        .last()
        .unwrap();
    let derivation = document.field(previous, "derivation").unwrap();
    let value_node = document.field(derivation, "value").unwrap();
    let expression = document.field(value_node, "row_value").unwrap();
    let window = document.field(expression, "window").unwrap();
    let order = document.field(window, "order_by").unwrap();
    let term = sequence(document, order)[0];
    let nulls = document.field(term, "nulls").unwrap();
    // Earlier layer materialization has already supplied nulls. Preserve the
    // actual normalization-input authority; do not infer a Default descriptor
    // origin from the equal text in a different schema arena.
    assert_eq!(prepared.origins()[nulls].source, SchemaSource::Input);
    assert_eq!(
        prepared.normalization_input().nodes()[prepared.origins()[nulls].node],
        N::Text("last".into())
    );
    assert_eq!(
        prepared.declaring_source("windows.VISITS.order_by[0].nulls"),
        Some("entry.yaml")
    );
    assert_eq!(
        host.calls
            .iter()
            .filter(|call| call.starts_with("capture:"))
            .count(),
        1
    );
}

#[test]
fn count_dependency_pruning_keeps_complete_normalized_application_model() {
    let authored = json!({
        "schema_version":"1.0","parents":[],"domain":"OUT","keys":["ID"],
        "input":{"DM":"dm.csv","EX":"ex.csv","DEAD":"never-read.csv"}, "base":"DM",
        "output":{"path":"out.csv","columns":["ID","N"]},
        "columns":[
            {"name":"ID","type":"str","derivation":"DM.ID"},
            {"name":"N","type":"int","derivation":{"aggregate":{"expr":"COUNT(EX.*)"}}},
            {"name":"UNUSED","type":"int","derivation":{"aggregate":{"expr":"COUNT(DEAD.*)"}}}
        ]
    });
    let mut host = new_host(b"unused");
    let prepared = shipped_schema::prepare(
        source(authored.to_string().into_bytes()),
        "entry.yaml".into(),
        &mut host,
    )
    .unwrap_or_else(|_| panic!("valid normalized count scope"));
    assert_eq!(
        value(prepared.model().document()),
        json!({
            "schema_version":"1.0","domain":"OUT","keys":["ID"],
            "input":{"DM":{"path":"dm.csv","empty_string":"missing"},"EX":{"path":"ex.csv","empty_string":"missing"}}, "base":"DM",
            "output":{"path":"out.csv","columns":["ID","N"]},
            "columns":[
                {"name":"ID","type":"str","derivation":{"value":{"source":{"variable":"DM.ID"}}}},
                {"name":"N","type":"int","derivation":{"value":{"aggregate":{"expr":"COUNT(EX.*)"}}}}
            ]
        })
    );
    assert_eq!(
        host.calls,
        [
            "rebase:entry.yaml:entry.yaml:never-read.csv:1048576",
            "rebase:entry.yaml:entry.yaml:dm.csv:1048562",
            "rebase:entry.yaml:entry.yaml:ex.csv:1048556",
            "rebase:entry.yaml:entry.yaml:out.csv:1048550"
        ]
    );
    assert_eq!(prepared.written_source_path("DM"), Some("dm.csv"));
    assert_eq!(prepared.written_source_path("EX"), Some("ex.csv"));
}

#[test]
fn explicit_null_handler_and_raw_origin_remain_distinct_from_inherited_clear() {
    let mut authored = window();
    authored["intermediates"] = json!([{"id":"LOOK","dataset":"SRC","key":["ID"],"no_match":null}]);
    let source = source(authored.to_string().into_bytes());
    let raw_pointer = source.bytes.as_ptr();
    let mut host = new_host(b"unused");
    let prepared = shipped_schema::prepare(source, "entry.yaml".into(), &mut host)
        .unwrap_or_else(|_| panic!("standalone literal null"));
    assert_eq!(prepared.source().bytes.as_ptr(), raw_pointer);
    assert!(host.calls.is_empty());
    let document = prepared.model().document();
    assert_eq!(
        value(document),
        json!({
            "schema_version":"1.0","domain":"OUT","base":"SRC","keys":["ID"],
            "input":{"SRC":{"path":"input.csv","types":{"SEQ":"int","VAL":"int"},"empty_string":"missing"}},
            "output":{"path":"out.csv","columns":["ID","PREV"]},
            "columns":[
                {"name":"ID","type":"str","label":"ID","derivation":{"value":{"source":{"variable":"SRC.ID"}}}},
                {"name":"G","type":"str","label":"G","derivation":{"value":{"source":{"variable":"SRC.G"}}}},
                {"name":"SEQ","type":"int","label":"SEQ","derivation":{"value":{"source":{"variable":"SRC.SEQ"}}}},
                {"name":"VAL","type":"int","label":"VAL","derivation":{"value":{"source":{"variable":"SRC.VAL"}}}},
                {"name":"PREV","type":"int","label":"Previous value","derivation":{"value":{"row_value":{"source":"VAL","offset":-1,"window":{"group_by":["G"],"order_by":[{"variable":"SEQ","direction":"asc","nulls":"last"}]}}}}}
            ],
            "intermediates":[{"id":"LOOK","dataset":"SRC","key":["ID"],"no_match":null}]
        })
    );
    let intermediate = sequence(document, path(document, &["intermediates"]))[0];
    let null = document.field(intermediate, "no_match").unwrap();
    assert_eq!(document.nodes()[null], N::Null);
    let raw = &prepared.raw().document;
    let raw_intermediate = sequence(raw, path(raw, &["intermediates"]))[0];
    assert_eq!(prepared.origins()[null].source, SchemaSource::Input);
    assert_eq!(
        prepared.origins()[null].node,
        raw.field(raw_intermediate, "no_match").unwrap()
    );
    assert!(!prepared.origins()[null].generated);
    assert_eq!(
        prepared.declaring_source("intermediates.LOOK.no_match"),
        Some("entry.yaml")
    );
    // Standalone expansion supplies the same default from its exact admitted
    // descriptor, unlike the already-materialized inherited input above.
    let previous = *sequence(document, path(document, &["columns"]))
        .last()
        .unwrap();
    let derivation = document.field(previous, "derivation").unwrap();
    let value_node = document.field(derivation, "value").unwrap();
    let expression = document.field(value_node, "row_value").unwrap();
    let window = document.field(expression, "window").unwrap();
    let order = document.field(window, "order_by").unwrap();
    let term = sequence(document, order)[0];
    let nulls = document.field(term, "nulls").unwrap();
    let descriptor = prepared
        .schema()
        .structure()
        .class_named("order_term_class")
        .unwrap()
        .fields
        .iter()
        .find(|field| field.name == "nulls")
        .unwrap()
        .descriptor;
    let admitted = &prepared.schema().structure().descriptors()[descriptor];
    assert_eq!(
        prepared.origins()[nulls].source,
        SchemaSource::Default {
            module: admitted.module,
            descriptor
        }
    );
    assert_eq!(
        prepared.origins()[nulls].node,
        admitted.descriptor.default_node().unwrap()
    );
    assert_eq!(
        prepared.schema().structure().modules()[admitted.module]
            .document
            .nodes()[prepared.origins()[nulls].node],
        N::Text("last".into())
    );
}

#[test]
fn distinct_written_aliases_keep_first_canonical_arena_and_complete_authored_model() {
    struct Aliases(Host);
    impl InheritancePort for Aliases {
        type Error = Payload;
        fn canonicalize(
            &mut self,
            declaring: &str,
            written: &str,
        ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
            self.0.calls.push(format!("resolve:{declaring}:{written}"));
            assert!(["base.yaml", "alias.yaml"].contains(&written));
            Ok(graph::Source {
                identity: "canonical/base.yaml".into(),
                display_path: format!("written/{written}"),
            })
        }
        fn capture(
            &mut self,
            source: &graph::Source,
            maximum: usize,
        ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
            self.0.capture(source, maximum)
        }
        fn rebase(
            &mut self,
            layer: &graph::Source,
            entry: &graph::Source,
            written: &str,
            maximum: usize,
        ) -> Result<String, Self::Error> {
            self.0.rebase(layer, entry, written, maximum)
        }
    }
    let original = json!({"schema_version":"1.0","domain":"OUT","keys":["ID"],"input":{"DM":"data.csv"},"base":"DM","output":{"path":"out.csv","columns":["ID"]},"columns":[{"name":"ID","type":"str","derivation":"DM.ID"}]}).to_string().into_bytes();
    let pointer = original.as_ptr();
    let entry = b"schema_version: '1.0'\nparents: [base.yaml, alias.yaml]\n".to_vec();
    let allowance = 16_777_216 - entry.len();
    let mut host = Aliases(Host {
        bytes: [("canonical/base.yaml".into(), original)]
            .into_iter()
            .collect(),
        calls: vec![],
        failure: None,
        fail_at: usize::MAX,
    });
    let prepared = shipped_schema::prepare(source(entry), "entry.yaml".into(), &mut host)
        .unwrap_or_else(|_| panic!("valid canonical alias"));
    assert_eq!(
        value(prepared.model().document()),
        json!({"schema_version":"1.0","domain":"OUT","keys":["ID"],"input":{"DM":{"path":"data.csv","empty_string":"missing"}},"base":"DM","output":{"path":"out.csv","columns":["ID"]},"columns":[{"name":"ID","type":"str","derivation":{"value":{"source":{"variable":"DM.ID"}}}}]})
    );
    assert_eq!(
        host.0.calls,
        [
            "resolve:entry.yaml:base.yaml".into(),
            format!("capture:canonical/base.yaml:{allowance}"),
            "resolve:entry.yaml:alias.yaml".into(),
            "rebase:canonical/base.yaml:entry.yaml:data.csv:1048576".into(),
            "rebase:canonical/base.yaml:entry.yaml:out.csv:1048568".into()
        ]
    );
    assert_eq!(prepared.parents().len(), 1);
    assert_eq!(prepared.parents()[0].source().bytes.as_ptr(), pointer);
    let inherited = prepared.inheritance().unwrap();
    assert_eq!(
        inherited
            .layers()
            .iter()
            .map(|layer| layer.source.identity.as_str())
            .collect::<Vec<_>>(),
        ["canonical/base.yaml", "entry.yaml"]
    );
    assert_eq!(
        inherited.layers()[0].source.display_path,
        "written/base.yaml"
    );
    let origin = prepared.written_input_origin("DM", "path").unwrap();
    assert_eq!(
        (origin.declaring_source, origin.written),
        ("canonical/base.yaml", "data.csv")
    );
    assert_eq!(
        prepared.declaring_source("input.DM.path"),
        Some("canonical/base.yaml")
    );
}
