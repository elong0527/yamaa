use serde_json::{json, Value};
use yamaa_adapters::{
    specification_diagnostics::capture_failure,
    specification_source::{CapturedSchema, Error, Source},
    yaml_decode::SourcePosition,
};
use yamaa_core::schema::{
    BundleError, BundleIssue, BundleIssueKind, DocumentNode as N, NormalizationError, SchemaSource,
};
const SHARED:&str="version: '1.0'\nincludes: []\nshared:\n  - id: {type: str, required: true}\n  - label: {type: str, default: untitled}\n";
const ROOT:&str="version: '1.0'\nincludes: [schema_s.yaml]\nroot_class:\n  - fields_from: shared\n  - schema_version: {type: str}\n";
fn source(identity: &str, bytes: &str) -> Source {
    Source {
        identity: identity.into(),
        bytes: bytes.as_bytes().to_vec(),
    }
}
fn closure() -> Vec<Source> {
    vec![source("schema_s.yaml", SHARED), source("r", ROOT)]
}
#[test]
fn original_sources_locations_reused_defaults_and_fresh_normalization_are_retained() {
    for _ in 0..2 {
        let sources = closure();
        let arena = sources.as_ptr();
        let bytes = sources.iter().map(|s| s.bytes.as_ptr()).collect::<Vec<_>>();
        let identities = sources
            .iter()
            .map(|s| s.identity.as_ptr())
            .collect::<Vec<_>>();
        let schema = CapturedSchema::admit(sources, 1).unwrap();
        assert_eq!(schema.sources().as_ptr(), arena);
        assert_eq!(
            schema
                .sources()
                .iter()
                .map(|s| s.identity.as_str())
                .collect::<Vec<_>>(),
            ["schema_s.yaml", "r"]
        );
        assert_eq!(schema.sources()[0].bytes, SHARED.as_bytes());
        assert_eq!(schema.sources()[1].bytes, ROOT.as_bytes());
        for i in 0..2 {
            assert_eq!(schema.sources()[i].bytes.as_ptr(), bytes[i]);
            assert_eq!(schema.sources()[i].identity.as_ptr(), identities[i]);
        }
        let descriptor = schema.structure().root_class().fields[1].descriptor;
        let field = &schema.structure().descriptors()[descriptor];
        assert_eq!((field.module, field.path.as_str()), (0, "root_class.label"));
        let node = field.descriptor.default_node().unwrap();
        let offset = SHARED.find("untitled").unwrap();
        let line_start = SHARED[..offset].rfind('\n').unwrap() + 1;
        assert_eq!(
            schema.locations()[0][node],
            SourcePosition {
                offset,
                line: 5,
                column: offset - line_start + 1
            }
        );
        assert_eq!(
            schema.structure().modules()[0].document.nodes()[node],
            N::Text("untitled".into())
        );
        let raw = source("domain.yaml", "schema_version: '1.0'\nid: ID01\n");
        let raw_pointer = raw.bytes.as_ptr();
        let prepared = schema.prepare_structural(raw).unwrap();
        assert_eq!(prepared.source().bytes.as_ptr(), raw_pointer);
        let d = &prepared.normalized().document;
        let label = d.field(d.root(), "label").unwrap();
        assert_eq!(d.nodes()[label], N::Text("untitled".into()));
        assert_eq!(
            prepared.normalized().origins[label].source,
            SchemaSource::Default {
                module: 0,
                descriptor
            }
        );
        assert_eq!(prepared.normalized().origins[label].node, node);
    }
}

#[test]
fn physical_decode_precedes_invalid_entry_and_earlier_bundle_semantics() {
    for entry in [0, usize::MAX] {
        let error = CapturedSchema::admit(
            vec![
                source("same", "ordinary scalar\n"),
                source("same", "[unclosed"),
                source("late", "[later unclosed"),
            ],
            entry,
        )
        .unwrap_err();
        assert!(matches!(error,Error::Decode{ref identity,..} if identity=="same"));
    }
    let error = CapturedSchema::admit(closure(), usize::MAX).unwrap_err();
    assert!(
        matches!(error,Error::Bundle(BundleError::Invalid(ref issues)) if issues==&[BundleIssue{module:usize::MAX,path:"$".into(),kind:BundleIssueKind::ModuleName}])
    );
}
#[test]
fn complete_original_codec_reports_survive_schema_closure_admission() {
    let mut count = 0;
    for row in serde_json::from_str::<Vec<Value>>(include_str!("../fixtures/yaml_transport.json"))
        .unwrap()
        .iter()
    {
        let mut expected: Value = row["expected"].clone();
        if expected["outcome"]["status"] != "invalid" {
            continue;
        }
        expected["protocol"] = json!("specification/prototype");
        for finding in expected["outcome"]["diagnostics"].as_array_mut().unwrap() {
            finding["phase"] = json!("validation");
            finding["requirement"] = json!(match finding["condition"].as_str().unwrap() {
                "non_ascii_source" => "REQ-0705",
                "invalid_text" => "REQ-0029",
                "invalid_yaml" => "REQ-0280",
                condition => panic!("unexpected original source condition: {condition}"),
            });
            if finding["condition"] == "non_ascii_source" {
                finding["context"]["path"] = json!("source.yaml");
            }
        }
        let bytes: Vec<u8> = if let Some(text) = row["source"].as_str() {
            text.as_bytes().to_vec()
        } else {
            let hex = row["source"]["hex"].as_str().unwrap();
            hex.as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                .collect()
        };
        let error = CapturedSchema::admit(
            vec![Source {
                identity: "source.yaml".into(),
                bytes,
            }],
            usize::MAX,
        )
        .unwrap_err();
        let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
        assert_eq!(actual, expected, "{}", row["id"].as_str().unwrap());
        count += 1;
    }
    assert_eq!(count, 8);
}
#[test]
fn complete_bundle_issue_order_and_later_invalid_default_gate_are_unchanged() {
    let error=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - first: {type: unknown_one}\n  - second: {type: unknown_two}\n")],0).unwrap_err();
    let Error::Bundle(ref bundle) = error else {
        panic!("full typed bundle failure expected")
    };
    assert_eq!(
        bundle,
        &BundleError::Invalid(vec![
            BundleIssue {
                module: 0,
                path: "root_class.first".into(),
                kind: BundleIssueKind::UnknownType("unknown_one".into())
            },
            BundleIssue {
                module: 0,
                path: "root_class.second".into(),
                kind: BundleIssueKind::UnknownType("unknown_two".into())
            },
        ])
    );
    let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
    assert_eq!(
        actual,
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"capture","code":"schema_bundle"}})
    );
    let schema=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - schema_version: {type: str}\n  - value: {type: int, default: invalid}\n")],0).unwrap();
    assert!(matches!(
        schema.prepare_structural(source("root.yaml", "schema_version: '1.0'\n")),
        Err(Error::Normalize(NormalizationError::Defaults(_)))
    ));
}

#[test]
fn fixed_yaml_file_size_boundary_precedes_physical_decoding() {
    use yamaa_adapters::yaml_decode::DecodeFailure;
    use yamaa_core::limits::{LimitExceeded, Limits};
    let prefix = "version: '1.0'\nincludes: []\nroot_class: [{schema_version: {type: str}}]\n#";
    for size in [
        Limits::SPEC_BYTES - 1,
        Limits::SPEC_BYTES,
        Limits::SPEC_BYTES + 1,
    ] {
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.resize(size, b'x');
        let result = CapturedSchema::admit(
            vec![Source {
                identity: "authored-schema.yaml".into(),
                bytes,
            }],
            0,
        );
        if size <= Limits::SPEC_BYTES {
            let schema = result.unwrap();
            assert_eq!(schema.sources()[0].bytes.len(), size);
            assert_eq!(schema.structure().root_class().name, "root_class");
        } else {
            assert!(
                matches!(result, Err(Error::Decode { identity, error: DecodeFailure::Limit(LimitExceeded {
                resource: "spec_bytes", limit: Limits::SPEC_BYTES, actual: Some(actual)
            }) }) if identity == "authored-schema.yaml" && actual == size)
            );
        }
    }
}
