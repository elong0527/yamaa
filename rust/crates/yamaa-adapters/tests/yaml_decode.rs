#![cfg(feature = "yaml")]
//! YAML decoding: the reader of `fixtures/yaml_decode.json`, whose cases each
//! state a source and the root mapping (as key and value pairs) or root
//! sequence it decodes to, every scalar a text scalar. The decoder is the
//! seam: a specification run reaches it only behind schema admission, which
//! would also judge the decoded values.
use serde_json::Value as Json;
use yamaa_adapters::yaml_decode::decode_yaml;
use yamaa_core::schema::DocumentNode;

/// A decoded root, in the shape a case states it.
#[derive(Debug, PartialEq)]
enum Root {
    Mapping(Vec<(String, String)>),
    Sequence(Vec<String>),
}

fn stated(name: &str, case: &Json) -> Root {
    let text = |value: &Json| value.as_str().unwrap().to_owned();
    match (case.get("mapping"), case.get("sequence")) {
        (Some(pairs), None) => Root::Mapping(
            pairs
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (text(&pair[0]), text(&pair[1])))
                .collect(),
        ),
        (None, Some(items)) => Root::Sequence(items.as_array().unwrap().iter().map(text).collect()),
        _ => panic!("{name}: state exactly one of mapping or sequence"),
    }
}

#[test]
fn every_case_decodes_to_its_stated_root() {
    let truth: Json = serde_json::from_str(include_str!("fixtures/yaml_decode.json")).unwrap();
    let cases = truth["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14, "yaml_decode.json case count");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let decoded = decode_yaml(source.as_bytes())
            .unwrap_or_else(|error| panic!("{name}: {error:?}"))
            .document;
        let nodes = decoded.nodes();
        let text = |index: usize| match &nodes[index] {
            DocumentNode::Text(text) => text.clone(),
            other => panic!("{name}: a text scalar, not {other:?}"),
        };
        let actual = match &nodes[decoded.root()] {
            DocumentNode::Mapping(pairs) => Root::Mapping(
                pairs
                    .iter()
                    .map(|&(key, value)| (text(key), text(value)))
                    .collect(),
            ),
            DocumentNode::Sequence(items) => {
                Root::Sequence(items.iter().map(|&item| text(item)).collect())
            }
            other => panic!("{name}: a mapping or sequence root, not {other:?}"),
        };
        assert_eq!(actual, stated(name, case), "{name}");
    }
}
