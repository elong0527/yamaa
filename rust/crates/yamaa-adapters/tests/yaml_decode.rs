#![cfg(feature = "yaml")]
//! YAML decoding: the reader of `fixtures/yaml_decode.json`, whose cases each
//! state a source and the root mapping it decodes to, every value a text
//! scalar. The decoder is the seam: a specification run reaches it only behind
//! schema admission, which would also judge the decoded values.
use serde_json::Value as Json;
use yamaa_adapters::yaml_decode::decode_yaml;
use yamaa_core::schema::DocumentNode;

#[test]
fn every_case_decodes_to_its_stated_root_mapping() {
    let truth: Json = serde_json::from_str(include_str!("fixtures/yaml_decode.json")).unwrap();
    let cases = truth["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7, "yaml_decode.json case count");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let expected: Vec<(String, String)> = case["mapping"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                let text = |index: usize| pair[index].as_str().unwrap().to_owned();
                (text(0), text(1))
            })
            .collect();
        let decoded = decode_yaml(source.as_bytes())
            .unwrap_or_else(|error| panic!("{name}: {error:?}"))
            .document;
        let nodes = decoded.nodes();
        let text = |index: usize| match &nodes[index] {
            DocumentNode::Text(text) => text.clone(),
            other => panic!("{name}: a text scalar, not {other:?}"),
        };
        let DocumentNode::Mapping(pairs) = &nodes[decoded.root()] else {
            panic!("{name}: the root is not a mapping");
        };
        let actual: Vec<_> = pairs
            .iter()
            .map(|&(key, value)| (text(key), text(value)))
            .collect();
        assert_eq!(actual, expected, "{name}");
    }
}
