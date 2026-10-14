use serde_json::{json, Value};
use yamaa_adapters::{
    specification_diagnostics::capture_failure,
    specification_source::{CapturedSchema, Error, Limits, Source},
    yaml_decode::SourcePosition,
};
use yamaa_core::schema::{
    BundleError, BundleIssue, BundleIssueKind, BundleLimits, DocumentNode as N, NormalizationError,
    SchemaSource,
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
        let schema = CapturedSchema::admit(sources, 1, Limits::default()).unwrap();
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
fn default_and_named_root_keep_their_distinct_original_text_gate() {
    let simple = || {
        vec![source(
            "r",
            "version: '1.0'\nincludes: []\nroot_class: [{schema_version: {type: str}}]\n",
        )]
    };
    let limits = Limits {
        identity_bytes: 1,
        ..Default::default()
    };
    assert!(CapturedSchema::admit(simple(), 0, limits).is_ok());
    assert!(matches!(
        CapturedSchema::admit_root(simple(), 0, "root_class", limits),
        Err(Error::Limit("root_class"))
    ));
    assert!(CapturedSchema::admit_root(
        simple(),
        0,
        "root_class",
        Limits {
            identity_bytes: 10,
            ..Default::default()
        }
    )
    .is_ok());
    let limits = Limits {
        captured_bytes: 0,
        identity_bytes: 0,
        bundle: BundleLimits {
            modules: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        CapturedSchema::admit_root(closure(), 1, "root_class", limits),
        Err(Error::Limit("root_class"))
    ));
    assert!(matches!(
        CapturedSchema::admit(closure(), 1, limits),
        Err(Error::Limit("schema_modules"))
    ));
}
#[test]
fn whole_closure_late_limits_and_byte_before_identity_keep_existing_categories() {
    let bytes = SHARED.len() + ROOT.len();
    assert!(CapturedSchema::admit(
        closure(),
        1,
        Limits {
            captured_bytes: bytes,
            identity_bytes: 14,
            ..Default::default()
        }
    )
    .is_ok());
    for (limits, expected) in [
        (
            Limits {
                captured_bytes: bytes - 1,
                identity_bytes: 14,
                ..Default::default()
            },
            "captured_bytes",
        ),
        (
            Limits {
                captured_bytes: bytes,
                identity_bytes: 13,
                ..Default::default()
            },
            "identity_bytes",
        ),
        (
            Limits {
                captured_bytes: 0,
                identity_bytes: 0,
                ..Default::default()
            },
            "captured_bytes",
        ),
    ] {
        let mut sources = closure();
        sources[0].bytes = b"[unclosed".to_vec();
        if expected == "captured_bytes" {
            sources[1].bytes = vec![b'x'; bytes];
        }
        let error = CapturedSchema::admit(sources, 1, limits).unwrap_err();
        assert!(matches!(error,Error::Limit(actual) if actual==expected));
        let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
        assert_eq!(
            actual,
            json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"capture","code":"capture_limit"}})
        );
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
            Limits::default(),
        )
        .unwrap_err();
        assert!(matches!(error,Error::Decode{ref identity,..} if identity=="same"));
    }
    let error = CapturedSchema::admit(closure(), usize::MAX, Limits::default()).unwrap_err();
    assert!(
        matches!(error,Error::Bundle(BundleError::Invalid(ref issues)) if issues==&[BundleIssue{module:usize::MAX,path:"$".into(),kind:BundleIssueKind::ModuleName}])
    );
}
#[test]
fn complete_original_codec_reports_survive_schema_closure_admission() {
    let mut count = 0;
    for line in include_str!("fixtures/yaml_transport.tsv").lines().skip(1) {
        let fields = line.split('\t').collect::<Vec<_>>();
        let mut expected: Value = serde_json::from_str(fields[2]).unwrap();
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
        let bytes = fields[1]
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        let error = CapturedSchema::admit(
            vec![Source {
                identity: "source.yaml".into(),
                bytes,
            }],
            usize::MAX,
            Limits::default(),
        )
        .unwrap_err();
        let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
        assert_eq!(actual, expected, "{}", fields[0]);
        count += 1;
    }
    assert_eq!(count, 8);
}
#[test]
fn complete_bundle_issue_order_and_later_invalid_default_gate_are_unchanged() {
    let error=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - first: {type: unknown_one}\n  - second: {type: unknown_two}\n")],0,Limits::default()).unwrap_err();
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
    let schema=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - schema_version: {type: str}\n  - value: {type: int, default: invalid}\n")],0,Limits::default()).unwrap();
    assert!(matches!(
        schema.prepare_structural(source("root.yaml", "schema_version: '1.0'\n")),
        Err(Error::Normalize(NormalizationError::Defaults(_)))
    ));
}
