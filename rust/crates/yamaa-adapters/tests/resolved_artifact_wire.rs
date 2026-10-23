//! The inspection reader works independently of optional physical readers.
use crate::support::Edit as _;
use yamaa_adapters::resolved_artifact::model::specification_value;
use yamaa_adapters::resolved_artifact::ResolvedArtifact;
use yamaa_core::schema::{Document, DocumentError, DocumentNode as N};

#[test]
fn inspection_round_trip_requires_no_yaml_or_parquet_reader() {
    const TEXT: &str = r#"{
      "format":"yamaa.resolved-specification", "artifact_version":1,
      "model_kind":"normalized_with_named_dictionary_references", "replay":"inspection_only",
      "entry":"/held/spec.yaml", "project":null,
      "entries":[{
        "entry":"/held/spec.yaml", "language_version":"1.0",
        "specification":{"yamaa":"1.0","examples":[123456789012345678901234567890,1.0]},
        "mapping_order":{"": ["yamaa","examples"]},
        "origins":{
          "":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/yamaa":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/examples":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/examples/0":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false},
          "/examples/1":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false}
        },
        "layers":["/held/spec.yaml"], "resources":[]
      }]
    }"#;
    let artifact = ResolvedArtifact::from_json(TEXT).unwrap();
    let serialized = artifact.to_json().unwrap();
    assert!(serialized.contains("123456789012345678901234567890,1.0"));
    assert_eq!(artifact, ResolvedArtifact::from_json(&serialized).unwrap());
    assert!(ResolvedArtifact::from_json(
        &serialized.edit("\"artifact_version\":1", "\"artifact_version\":2")
    )
    .is_err());
}

#[test]
fn numeric_projection_preserves_admitted_values_and_malformed_documents_fail_admission() {
    // Document exposes only admitted immutable arenas: malformed numbers fail
    // before they can reach the public artifact projection utility.
    for text in ["", "invalid", "1.2", "+1", "01", "-0"] {
        assert!(matches!(
            Document::new(vec![N::Integer(text.into())], 0),
            Err(DocumentError::InvalidInteger)
        ));
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            Document::new(vec![N::Float(value)], 0),
            Err(DocumentError::NonFinite)
        ));
    }

    let document = Document::new(
        vec![
            N::Integer("123456789012345678901234567890".into()),
            N::Integer("-123456789012345678901234567890".into()),
            N::Float(f64::MAX),
            N::Float(f64::MIN),
            N::Float(f64::MIN_POSITIVE),
            N::Float(f64::from_bits(1)),
            N::Float(-0.0),
            N::Sequence((0..7).collect()),
        ],
        7,
    )
    .unwrap();
    let (value, mapping_order) = specification_value(&document).unwrap();
    assert!(mapping_order.is_empty());
    assert_eq!(
        yamaa_adapters::json_wire::encode(&value),
        "[123456789012345678901234567890,-123456789012345678901234567890,1.7976931348623157e308,-1.7976931348623157e308,2.2250738585072014e-308,5e-324,-0.0]"
    );
}
