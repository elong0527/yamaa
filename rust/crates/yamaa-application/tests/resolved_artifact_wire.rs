//! The inspection reader works independently of optional physical readers.
//!
//! The seam is `ResolvedArtifact`'s JSON wire, read and written by
//! `from_json` and `to_json`, and `specification_value` over core `Document`
//! arenas built by code: no case operation returns a resolved artifact.
use crate::support::Edit as _;
use yamaa_application::resolved_artifact::model::{self, specification_value};
use yamaa_application::resolved_artifact::ResolvedArtifact;
use yamaa_core::schema::{Document, DocumentError, DocumentNode as N};

/// A version 1 artifact as an engine before 0.7.0 wrote it: no origin
/// carries a line or a column.
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

#[test]
fn inspection_round_trip_requires_no_yaml_or_parquet_reader() {
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
fn an_origin_reads_with_the_line_and_column_its_layer_wrote_or_without_either() {
    // The seam is the wire's origin ledger, which version 1 extended with an
    // optional location: an authored origin carries a written line and
    // column, both one-based, or neither, as an earlier engine wrote it.
    const WRITTEN: &str = r#""/yamaa":{"declaring_source":"/held/spec.yaml","source_kind":"authored","generated":false}"#;
    let origin = |fields: &str| format!(r#""/yamaa":{{{fields},"generated":false}}"#);
    let authored = |location: &str| {
        origin(&format!(
            r#""declaring_source":"/held/spec.yaml",{location}"source_kind":"authored""#
        ))
    };
    let artifact =
        ResolvedArtifact::from_json(&TEXT.edit(WRITTEN, &authored(r#""line":1,"column":8,"#)))
            .unwrap();
    let value = artifact.to_value().unwrap();
    let origins = &value["entries"][0]["origins"];
    assert_eq!(
        (&origins["/yamaa"], &origins["/examples"]),
        (
            &yamaa_adapters::json!({
                "declaring_source": "/held/spec.yaml", "line": 1, "column": 8,
                "source_kind": "authored", "generated": false
            }),
            &yamaa_adapters::json!({
                "declaring_source": "/held/spec.yaml",
                "source_kind": "authored", "generated": false
            }),
        ),
        "a located origin and one without a location, written back"
    );
    assert_eq!(
        ResolvedArtifact::from_json(&artifact.to_json().unwrap()).unwrap(),
        artifact
    );
    for (label, location, end) in [
        (
            "exclusive scalar end",
            r#""line":1,"column":8,"end_line":1,"end_column":13,"#,
            yamaa_adapters::json!({"end_line":1,"end_column":13}),
        ),
        (
            "multiline node end",
            r#""line":1,"column":8,"end_line":3,"end_column":1,"#,
            yamaa_adapters::json!({"end_line":3,"end_column":1}),
        ),
        (
            "empty node",
            r#""line":1,"column":8,"end_line":1,"end_column":8,"#,
            yamaa_adapters::json!({"end_line":1,"end_column":8}),
        ),
    ] {
        let artifact =
            ResolvedArtifact::from_json(&TEXT.edit(WRITTEN, &authored(location))).unwrap();
        let mut expected = yamaa_adapters::json!({
            "declaring_source":"/held/spec.yaml", "line":1,"column":8,
            "source_kind":"authored","generated":false
        });
        expected
            .as_object_mut()
            .unwrap()
            .extend(end.as_object().unwrap().clone());
        assert_eq!(
            artifact.to_value().unwrap()["entries"][0]["origins"]["/yamaa"],
            expected,
            "{label}"
        );
        assert_eq!(
            ResolvedArtifact::from_json(&artifact.to_json().unwrap()).unwrap(),
            artifact,
            "{label}"
        );
    }
    let default = |location: &str| {
        origin(&format!(
            r#""declaring_source":"yamaa:schema",{location}"source_kind":"schema_default""#
        ))
    };
    for (label, refused) in [
        ("a line without a column", authored(r#""line":1,"#)),
        ("a column without a line", authored(r#""column":8,"#)),
        ("line zero", authored(r#""line":0,"column":8,"#)),
        ("column zero", authored(r#""line":1,"column":0,"#)),
        (
            "an end line without its column",
            authored(r#""line":1,"column":8,"end_line":1,"#),
        ),
        (
            "an end column without its line",
            authored(r#""line":1,"column":8,"end_column":13,"#),
        ),
        (
            "an end without a start",
            authored(r#""end_line":1,"end_column":13,"#),
        ),
        (
            "end line zero",
            authored(r#""line":1,"column":8,"end_line":0,"end_column":13,"#),
        ),
        (
            "end column zero",
            authored(r#""line":1,"column":8,"end_line":1,"end_column":0,"#),
        ),
        (
            "an earlier end line",
            authored(r#""line":2,"column":8,"end_line":1,"end_column":13,"#),
        ),
        (
            "an earlier end column",
            authored(r#""line":1,"column":8,"end_line":1,"end_column":7,"#),
        ),
        (
            "an end on a default",
            default(r#""line":1,"column":8,"end_line":1,"end_column":13,"#),
        ),
        (
            "a located schema default",
            default(r#""line":1,"column":8,"#),
        ),
        (
            "a located composed value",
            origin(r#""declaring_source":null,"line":1,"column":8,"source_kind":"composed""#),
        ),
    ] {
        assert!(
            matches!(
                ResolvedArtifact::from_json(&TEXT.edit(WRITTEN, &refused)),
                Err(model::Error::Facts("origin location is not a written one"))
            ),
            "{label}"
        );
    }
    // Without a location, the same schema default reads.
    assert!(ResolvedArtifact::from_json(&TEXT.edit(WRITTEN, &default(""))).is_ok());
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
