//! Existing YAML representation and independent complete outcomes around engine delegation.
use serde_json::{json, Value};
use std::sync::Arc;
use yamaa_adapters::{
    specification_diagnostics::capture_failure,
    specification_source::{
        CapturedSchema, Error, InheritanceError, InheritancePort, Limits, Source,
    },
    yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits},
};
use yamaa_core::schema::{
    DocumentLimits, DocumentNode as N, NormalizationError, NormalizationResource, SchemaSource,
};
use yamaa_engine::inheritance::{Source as Identity, SourceError};

const SCHEMA: &str = "version: '1.0'\nroot_class:\n  - schema_version: {type: str}\n  - domain: {type: [str, int], default: DEFAULT}\n  - input: {type: dict}\n  - keys: {type: 'list[str]'}\n  - output: {type: dict}\n  - columns: {type: 'list[column_class]'}\n  - windows: {type: 'dict[str, window_spec]'}\ncolumn_class:\n  - name: {type: str}\n  - type: {type: str}\n  - derivation: {type: handled_expression_class}\nhandled_expression_class:\n  - value: {type: expression}\nexpression: {registry: expressions}\nexpressions:\n  row_number:\n    - window: {type: window_selection}\nwindow_selection: {type: [str, window_spec]}\nwindow_spec:\n  - group_by: {type: 'list[str]'}\n  - order_by: {type: 'list[order_term]'}\norder_term:\n  - variable: {type: str}\n";
const VALID: &str = "schema_version: '1.0'\ndomain: X\ninput: {}\nkeys: []\noutput: {path: output.csv, columns: []}\ncolumns: []\n";
fn source(text: &str) -> Source {
    Source {
        identity: "retained/spec.yaml".into(),
        bytes: text.as_bytes().to_vec(),
    }
}
fn schema(limits: Limits) -> Arc<CapturedSchema> {
    CapturedSchema::admit(
        vec![Source {
            identity: "schema.yaml".into(),
            bytes: SCHEMA.as_bytes().to_vec(),
        }],
        0,
        limits,
    )
    .unwrap()
}
fn outcome(error: &Error) -> Value {
    serde_json::from_str(&capture_failure(error)).unwrap()
}
fn invalid(diagnostics: Value) -> Value {
    json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":diagnostics}})
}
struct NoParents;
impl InheritancePort for NoParents {
    type Error = std::rc::Rc<usize>;
    fn canonicalize(&mut self, _: &str, _: &str) -> Result<Identity, SourceError<Self::Error>> {
        panic!("standalone path must not canonicalize")
    }
    fn capture(&mut self, _: &Identity, _: usize) -> Result<Vec<u8>, SourceError<Self::Error>> {
        panic!("standalone path must not capture")
    }
    fn rebase(
        &mut self,
        _: &Identity,
        _: &Identity,
        _: &str,
        _: usize,
    ) -> Result<String, Self::Error> {
        panic!("standalone path must not rebase")
    }
}

#[test]
fn adapter_moves_source_allocations_and_retains_original_physical_locations_and_defaults() {
    let text = VALID.replace("domain: X\n", "");
    let entry = source(&text);
    let bytes = entry.bytes.as_ptr();
    let identity = entry.identity.as_ptr();
    let schema = schema(Limits::default());
    let prepared = schema.prepare_standalone(entry).unwrap();
    assert_eq!(prepared.source().bytes.as_ptr(), bytes);
    assert_eq!(prepared.source().identity.as_ptr(), identity);
    assert_eq!(prepared.source().bytes, text.as_bytes());
    let decoded = decode_yaml(text.as_bytes(), DecodeLimits::default()).unwrap();
    assert_eq!(prepared.raw().document, decoded.document);
    assert_eq!(prepared.raw().locations, decoded.locations);
    assert_eq!(
        prepared.model().document().nodes()[prepared
            .model()
            .document()
            .field(prepared.model().document().root(), "domain")
            .unwrap()],
        N::Text("DEFAULT".into())
    );
    let descriptor = schema
        .structure()
        .class_named("root_class")
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == "domain")
        .unwrap()
        .descriptor;
    let default = schema.structure().descriptors()[descriptor]
        .descriptor
        .default_node()
        .unwrap();
    let domain = prepared
        .model()
        .document()
        .field(prepared.model().document().root(), "domain")
        .unwrap();
    assert_eq!(
        prepared.origins()[domain].source,
        SchemaSource::Default {
            module: 0,
            descriptor
        }
    );
    assert_eq!(prepared.origins()[domain].node, default);
    assert!(!prepared.origins()[domain].generated);
    assert!(prepared.parents().is_empty());
    assert!(prepared.windows().is_empty());
    assert!(prepared.inheritance().is_none());
}

#[test]
fn both_semantic_passes_keep_source_identity_and_independent_complete_reports() {
    let schema = schema(Limits::default());
    let text = VALID.replace("'1.0'", "92233720368547758081234567890");
    let entry = source(&text);
    let bytes = entry.bytes.as_ptr();
    let identity = entry.identity.as_ptr();
    let error = schema.prepare_standalone(entry).unwrap_err();
    let Error::Findings(captured) = &error else {
        panic!()
    };
    assert_eq!(captured.source().bytes.as_ptr(), bytes);
    assert_eq!(captured.source().identity.as_ptr(), identity);
    let decoded = decode_yaml(text.as_bytes(), DecodeLimits::default()).unwrap();
    assert_eq!(captured.document(), Some(&decoded.document));
    let expected:Value=serde_json::from_str("{\"phase\":\"validation\",\"condition\":\"schema_version_mismatch\",\"requirement\":\"REQ-0281\",\"spec_paths\":[\"schema_version\"],\"context\":{\"expected\":\"1.0\",\"actual\":92233720368547758081234567890}}").unwrap();
    assert_eq!(outcome(&error), invalid(json!([expected])));
    let text="schema_version: '1.0'\ninput: {}\nkeys: []\noutput: {}\ncolumns:\n  - {name: Z, type: int, derivation: {value: {row_number: {window: SECOND}}}}\n  - {name: A, type: int, derivation: {value: {row_number: {window: FIRST}}}}\n";
    let entry = source(text);
    let bytes = entry.bytes.as_ptr();
    let identity = entry.identity.as_ptr();
    let error = schema.prepare_standalone(entry).unwrap_err();
    let Error::Findings(captured) = &error else {
        panic!()
    };
    assert_eq!(captured.source().bytes.as_ptr(), bytes);
    assert_eq!(captured.source().identity.as_ptr(), identity);
    let normalized = captured.document().unwrap();
    assert_eq!(
        normalized.nodes()[normalized.field(normalized.root(), "domain").unwrap()],
        N::Text("DEFAULT".into())
    );
    assert_ne!(
        normalized,
        &decode_yaml(text.as_bytes(), DecodeLimits::default())
            .unwrap()
            .document
    );
    assert_eq!(
        outcome(&error),
        invalid(json!([
            {"phase":"validation","condition":"unknown_window","requirement":"REQ-1253","spec_paths":["columns.Z.derivation.row_number.window"],"context":{"window":"SECOND"}},
            {"phase":"validation","condition":"unknown_window","requirement":"REQ-1253","spec_paths":["columns.A.derivation.row_number.window"],"context":{"window":"FIRST"}}
        ]))
    );
}

#[test]
fn final_model_findings_retain_no_pass_document_and_move_original_source() {
    let text = VALID
        .replace("domain: X", "domain: 7")
        .replace("{path: output.csv, columns: []}", "{}");
    let entry = source(&text);
    let pointer = entry.bytes.as_ptr();
    let error = schema(Limits::default())
        .prepare_standalone(entry)
        .unwrap_err();
    let Error::Findings(captured) = &error else {
        panic!()
    };
    assert!(captured.document().is_none());
    assert_eq!(captured.source().bytes.as_ptr(), pointer);
    assert_eq!(
        outcome(&error),
        invalid(json!([
            {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"reason":"Input should be a valid string"}},
            {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0285","spec_paths":["output.path"],"context":{"reason":"Field required"}},
            {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0285","spec_paths":["output.columns"],"context":{"reason":"Field required"}}
        ]))
    );
}

#[test]
fn physical_decode_and_source_limits_keep_their_existing_precedence_before_lifecycle() {
    let mut limits = Limits::default();
    limits.normalization.storage.nodes = 0;
    let schema = schema(limits);
    let text = VALID.to_owned() + "parents: null\n";
    assert!(matches!(
        schema.prepare_standalone(source(&text)),
        Err(Error::InheritanceRequired)
    ));
    assert!(matches!(
        schema.prepare_standalone(source("parents: null\n[unterminated")),
        Err(Error::Decode {
            error: DecodeFailure::InvalidYaml { .. },
            ..
        })
    ));
    let mut entry = source("[unterminated");
    entry.bytes.resize(limits.captured_bytes + 1, b'x');
    entry.identity = "x".repeat(limits.identity_bytes + 1);
    assert!(matches!(
        schema.prepare_standalone(entry),
        Err(Error::Limit("captured_bytes"))
    ));
    let mut entry = source("[unterminated");
    entry.identity = "x".repeat(limits.identity_bytes + 1);
    assert!(matches!(
        schema.prepare_standalone(entry),
        Err(Error::Limit("identity_bytes"))
    ));
}

#[test]
fn standalone_dispatch_has_no_parent_effect_and_shared_storage_boundary_is_unchanged() {
    for nodes in [33, 34] {
        let mut limits = Limits::default();
        limits.normalization.storage = DocumentLimits {
            nodes,
            ..Default::default()
        };
        let schema = schema(limits);
        let result =
            schema.prepare_document(source(VALID), "retained/spec.yaml".into(), &mut NoParents);
        if nodes == 34 {
            assert!(result.is_ok());
        } else {
            let Err(InheritanceError::Entry(Error::Normalize(error))) = result else {
                panic!()
            };
            assert_eq!(
                error,
                NormalizationError::Limit {
                    resource: NormalizationResource::Nodes,
                    limit: 33
                }
            );
        }
    }
    let error = schema(Limits::default())
        .prepare_document(
            source(&VALID.replace("'1.0'", "'99.0'")),
            "retained/spec.yaml".into(),
            &mut NoParents,
        )
        .unwrap_err();
    assert!(matches!(error, InheritanceError::Entry(Error::Findings(_))));
}

#[test]
fn schema_default_checkpoint_precedes_authored_version_and_window_errors() {
    let invalid = SCHEMA.replace(
        "type: [str, int], default: DEFAULT",
        "type: str, default: 7",
    );
    let schema = CapturedSchema::admit(
        vec![Source {
            identity: "schema.yaml".into(),
            bytes: invalid.into_bytes(),
        }],
        0,
        Limits::default(),
    )
    .unwrap();
    let entry = source("schema_version: '99.0'\nwindows: {BAD: broken}\n");
    let Error::Normalize(NormalizationError::Defaults(defaults)) =
        schema.prepare_standalone(entry).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].diagnostics.len(), 1);
    assert_eq!(
        defaults[0].diagnostics[0].definition().condition,
        "invalid_field_type"
    );
    // This remains an admitted-schema default failure, not a language finding
    // addressing the authored entry or a strict-window/model failure.
    let descriptor = &schema.structure().descriptors()[defaults[0].descriptor];
    assert_eq!(
        descriptor
            .descriptor
            .default_node()
            .map(|node| &schema.structure().modules()[descriptor.module]
                .document
                .nodes()[node]),
        Some(&N::Integer("7".into()))
    );
}
