//! Independent logical arenas and opaque effects exercise capture ownership.
use std::rc::Rc;
use yamaa_core::resource::ResourceFailure;
use yamaa_core::schema::{
    BundleLimits, Document, DocumentLimits, DocumentNode as N, SchemaModule, SchemaStructure,
    SpecificationDocument, ValidationBudget, ValidationLimits,
};
use yamaa_engine::{
    dictionary_preparation as app,
    inheritance::{Source as Identity, SourceError},
    inheritance_capture::{CapturePort, CaptureResource, DecodePort},
    project_preparation::Source,
};
#[derive(Clone)]
enum V {
    Text(&'static str),
    Bool(bool),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Bool, List, Map, Text};
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(value) => N::Text(value.into()),
            Bool(value) => N::Boolean(value),
            List(values) => {
                N::Sequence(values.into_iter().map(|value| push(value, nodes)).collect())
            }
            Map(values) => N::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| (push(Text(key), nodes), push(value, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn field(name: &'static str, kind: &'static str) -> V {
    Map(vec![(name, Map(vec![("type", Text(kind))]))])
}
fn union(name: &'static str, kinds: &[&'static str]) -> V {
    Map(vec![(
        name,
        Map(vec![(
            "type",
            List(kinds.iter().map(|kind| Text(kind)).collect()),
        )]),
    )])
}
fn schema() -> SchemaStructure {
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", Text("1.0")),
                (
                    "root_class",
                    List(vec![
                        field("schema_version", "str"),
                        field("domain", "str"),
                        field("input", "dict"),
                        field("keys", "list[str]"),
                        field("output", "dict"),
                        field("columns", "list[column_class]"),
                        field("rows", "list[row_class]"),
                        field("metadata", "dict"),
                    ]),
                ),
                (
                    "column_class",
                    List(vec![
                        field("name", "str"),
                        field("type", "str"),
                        field("derivation", "handled_expression_class"),
                    ]),
                ),
                (
                    "row_class",
                    List(vec![
                        field("id", "str"),
                        field("derivations", "dict[str, handled_expression_class]"),
                    ]),
                ),
                (
                    "handled_expression_class",
                    List(vec![
                        field("value", "expression"),
                        field("unconvertible", "literal_value"),
                    ]),
                ),
                ("expression", Map(vec![("registry", Text("expressions"))])),
                ("project_path", Map(vec![("type", Text("str"))])),
                (
                    "literal_value",
                    Map(vec![(
                        "type",
                        List(vec![
                            Text("str"),
                            Text("int"),
                            Text("float"),
                            Text("bool"),
                            Text("null"),
                        ]),
                    )]),
                ),
                (
                    "branch_class",
                    List(vec![field("when", "str"), field("value", "expression")]),
                ),
                (
                    "expressions",
                    Map(vec![
                        (
                            "mapping",
                            List(vec![
                                field("source", "str"),
                                union("dict", &["dict[str, literal_value]", "project_path"]),
                            ]),
                        ),
                        ("case", List(vec![field("branches", "list[branch_class]")])),
                        ("nested", List(vec![field("values", "list[expression]")])),
                    ]),
                ),
            ])),
        }],
        0,
        "root_class",
        BundleLimits::default(),
    )
    .unwrap()
}
fn mapping(dictionary: V) -> V {
    Map(vec![(
        "mapping",
        Map(vec![("source", Text("SRC.X")), ("dict", dictionary)]),
    )])
}
fn handled(value: V) -> V {
    Map(vec![("value", value)])
}
fn column(name: &'static str, value: V) -> V {
    Map(vec![
        ("name", Text(name)),
        ("type", Text("str")),
        ("derivation", handled(value)),
    ])
}
fn input(columns: Vec<V>, rows: Vec<V>) -> V {
    Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("DM")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("src.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                ("columns", List(vec![Text("X")])),
            ]),
        ),
        ("columns", List(columns)),
        ("rows", List(rows)),
    ])
}

#[derive(Debug)]
struct Host {
    tag: &'static str,
    owner: Rc<()>,
}
struct Port {
    events: Vec<String>,
    fail: Option<&'static str>,
    owner: Rc<()>,
}
impl Port {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            fail: None,
            owner: Rc::new(()),
        }
    }
}
impl CapturePort for Port {
    type Error = Host;
    fn canonicalize(&mut self, _: &str, _: &str) -> Result<Identity, SourceError<Host>> {
        panic!("parent route entered")
    }
    fn capture(&mut self, _: &Identity, _: usize) -> Result<Vec<u8>, SourceError<Host>> {
        panic!("parent route entered")
    }
    fn canonicalize_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<Host>> {
        self.events.push(format!("resolve:{declaring}:{written}"));
        if let Some(tag) = self.fail {
            return Err(SourceError::Raised(Host {
                tag,
                owner: self.owner.clone(),
            }));
        }
        Ok(Identity {
            identity: if written == "later.yaml" {
                "later"
            } else {
                "dictionary"
            }
            .into(),
            display_path: written.into(),
        })
    }
    fn capture_resource(
        &mut self,
        source: &Identity,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<Host>> {
        self.events
            .push(format!("capture:{}:{maximum}", source.identity));
        Ok(vec![if source.identity == "later" { 2 } else { 1 }])
    }
    fn resource_failure(&self, error: &Host) -> Option<ResourceFailure> {
        match error.tag {
            "missing" => Some(ResourceFailure::Missing),
            "directory" => Some(ResourceFailure::NotRegularFile),
            _ => None,
        }
    }
    fn rebase(&mut self, _: &Identity, _: &Identity, _: &str, _: usize) -> Result<String, Host> {
        panic!("rebase entered")
    }
}
#[derive(Debug, PartialEq)]
enum DecodeError {
    Limit(CaptureResource),
    Invalid,
}
struct Decoder {
    events: Vec<String>,
    invalid: bool,
}
impl Decoder {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            invalid: false,
        }
    }
}
impl DecodePort for Decoder {
    type Raw = Document;
    type Error = DecodeError;
    fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<Document, DecodeError> {
        self.events.push(format!("{identity}:{bytes:?}"));
        if self.invalid {
            return Err(DecodeError::Invalid);
        }
        Ok(if bytes == [2] {
            document(List(vec![Text("invalid dictionary")]))
        } else {
            document(Map(vec![("A", Text("answer")), ("B", Bool(false))]))
        })
    }
    fn document(raw: &Document) -> &Document {
        raw
    }
    fn limit_error(resource: CaptureResource) -> DecodeError {
        DecodeError::Limit(resource)
    }
}
fn model(paths: &[&'static str]) -> SpecificationDocument {
    let doc = document(input(
        paths
            .iter()
            .enumerate()
            .map(|(i, path)| column(if i == 0 { "X" } else { "Y" }, mapping(Text(path))))
            .collect(),
        vec![],
    ));
    SpecificationDocument::admit(doc, &mut ValidationBudget::new(ValidationLimits::default()))
        .unwrap()
        .unwrap()
}
#[test]
fn aliases_resolve_in_use_order_but_capture_and_parse_once_without_mutating_original() {
    let schema = schema();
    let model = model(&["dict.yaml", "./dict.yaml"]);
    let original = model.document().clone();
    let mut port = Port::new();
    let mut decoder = Decoder::new();
    let prepared = app::prepare(
        &schema,
        &model,
        app::Origins::Standalone("parent/base.yaml"),
        &[],
        &mut port,
        &mut decoder,
        app::Limits::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        port.events,
        vec![
            "resolve:parent/base.yaml:dict.yaml",
            "capture:dictionary:16777216",
            "resolve:parent/base.yaml:./dict.yaml"
        ]
    );
    assert_eq!(decoder.events, vec!["dictionary:[1]"]);
    assert_eq!(prepared.captures.len(), 1);
    assert_eq!(prepared.captures[0].source().bytes, [1]);
    assert_eq!(model.document(), &original);
    assert_ne!(prepared.model.document(), model.document());
}
#[test]
fn already_decoded_seed_identity_is_reused_without_capture_or_decode() {
    let schema = schema();
    let model = model(&["dict.yaml"]);
    let mut port = Port::new();
    let mut decoder = Decoder::new();
    let source = Source {
        identity: "dictionary".into(),
        bytes: vec![1],
    };
    let raw = document(Map(vec![("A", Text("answer"))]));
    let prepared = app::prepare(
        &schema,
        &model,
        app::Origins::Standalone("entry"),
        &[app::Seed {
            source: &source,
            raw: &raw,
        }],
        &mut port,
        &mut decoder,
        app::Limits::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(port.events, ["resolve:entry:dict.yaml"]);
    assert!(decoder.events.is_empty());
    assert!(prepared.captures.is_empty());
}
#[test]
fn classified_and_opaque_resource_errors_keep_original_payload_and_stop_without_retry() {
    for (tag, failure) in [
        ("missing", Some(ResourceFailure::Missing)),
        ("directory", Some(ResourceFailure::NotRegularFile)),
        ("opaque", None),
    ] {
        let mut port = Port::new();
        port.fail = Some(tag);
        let mut decoder = Decoder::new();
        let rejected = app::prepare(
            &schema(),
            &model(&["dict.yaml", "later.yaml"]),
            app::Origins::Standalone("entry"),
            &[],
            &mut port,
            &mut decoder,
            app::Limits::default(),
        )
        .unwrap_err();
        let app::Cause::Capture {
            error: SourceError::Raised(error),
            failure: actual,
            reference,
        } = &rejected.cause
        else {
            panic!("failure lost")
        };
        assert_eq!(error.tag, tag);
        assert!(Rc::ptr_eq(&error.owner, &port.owner));
        assert_eq!(*actual, failure);
        assert_eq!(reference.path(), "columns.X.derivation.mapping.dict");
        assert_eq!(reference.written(), "dict.yaml");
        assert!(rejected.captures.is_empty());
        assert_eq!(port.events, ["resolve:entry:dict.yaml"]);
        assert!(decoder.events.is_empty());
    }
}
#[test]
fn dictionary_contract_failure_retains_exact_first_capture_prefix_and_arena() {
    let mut port = Port::new();
    let mut decoder = Decoder::new();
    let rejected = app::prepare(
        &schema(),
        &model(&["dict.yaml", "later.yaml"]),
        app::Origins::Standalone("entry"),
        &[],
        &mut port,
        &mut decoder,
        app::Limits::default(),
    )
    .unwrap_err();
    let app::Cause::Admission {
        arena: app::Arena::Captured(1),
        reference,
        ..
    } = &rejected.cause
    else {
        panic!("raw context lost")
    };
    assert_eq!(reference.path(), "columns.Y.derivation.mapping.dict");
    assert_eq!(rejected.captures.len(), 2);
    assert_eq!(
        rejected.captures[1].raw().nodes()[rejected.captures[1].raw().root()],
        N::Sequence(vec![0])
    );
    assert_eq!(decoder.events, ["dictionary:[1]", "later:[2]"]);
}
#[test]
fn decode_failure_retains_entered_bytes_without_clone_and_no_later_capture() {
    let mut port = Port::new();
    let mut decoder = Decoder::new();
    decoder.invalid = true;
    let rejected = app::prepare(
        &schema(),
        &model(&["dict.yaml", "later.yaml"]),
        app::Origins::Standalone("entry"),
        &[],
        &mut port,
        &mut decoder,
        app::Limits::default(),
    )
    .unwrap_err();
    let app::Cause::Decode {
        source,
        error: DecodeError::Invalid,
        ..
    } = &rejected.cause
    else {
        panic!()
    };
    assert_eq!(source.bytes, [1]);
    assert!(rejected.captures.is_empty());
    assert_eq!(decoder.events, ["dictionary:[1]"]);
}
#[test]
fn cumulative_entry_and_dictionary_bytes_refuse_without_materialized_authority() {
    let mut port = Port::new();
    let mut decoder = Decoder::new();
    let source = Source {
        identity: "entry".into(),
        bytes: vec![0, 0],
    };
    let raw = document(Map(vec![]));
    let rejected = app::prepare(
        &schema(),
        &model(&["dict.yaml"]),
        app::Origins::Standalone("entry"),
        &[app::Seed {
            source: &source,
            raw: &raw,
        }],
        &mut port,
        &mut decoder,
        app::Limits {
            captured_bytes: 2,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        rejected.cause,
        app::Cause::Limit(DecodeError::Limit(CaptureResource::CapturedBytes))
    ));
    assert!(decoder.events.is_empty());
    assert_eq!(
        port.events,
        ["resolve:entry:dict.yaml", "capture:dictionary:0"]
    );
}
