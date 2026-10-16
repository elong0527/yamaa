//! The inspection reader works independently of optional physical readers.
use yamaa_adapters::resolved_artifact::ResolvedArtifact;

#[test]
fn inspection_round_trip_requires_no_yaml_or_parquet_reader() {
    const TEXT: &str = r#"{
      "format":"yamaa.resolved-specification", "artifact_version":1,
      "model_kind":"normalized_with_named_dictionary_references", "replay":"inspection_only",
      "entry":"/held/spec.yaml", "environment":null,
      "entries":[{
        "entry":"/held/spec.yaml", "schema_version":"1.0",
        "specification":{"schema_version":"1.0","examples":[123456789012345678901234567890,1.0]},
        "mapping_order":{"": ["schema_version","examples"]},
        "origins":{
          "":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/schema_version":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/examples":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/examples/0":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/examples/1":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false}
        },
        "layers":["/held/spec.yaml"], "resources":[], "producers":{}
      }]
    }"#;
    let artifact = ResolvedArtifact::from_json(TEXT).unwrap();
    let serialized = artifact.to_json().unwrap();
    assert!(serialized.contains("123456789012345678901234567890,1.0"));
    assert_eq!(artifact, ResolvedArtifact::from_json(&serialized).unwrap());
    assert!(ResolvedArtifact::from_json(
        &serialized.replace("\"artifact_version\":1", "\"artifact_version\":2")
    )
    .is_err());
}
