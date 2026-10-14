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
use yamaa_core::schema::{DocumentNode as N, NormalizationError, SchemaSource};
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
    let prepared = schema.dispatch(entry).unwrap();
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
    let error = schema.dispatch(entry).unwrap_err();
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
    let error = schema.dispatch(entry).unwrap_err();
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
    let error = schema(Limits::default()).dispatch(entry).unwrap_err();
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

// Exercise the live dispatch gate, while preserving its standalone representation.
trait Dispatch {
    fn dispatch(
        self: &Arc<Self>,
        source: Source,
    ) -> Result<yamaa_adapters::specification_source::PreparedDocument, Error>;
}
impl Dispatch for CapturedSchema {
    fn dispatch(
        self: &Arc<Self>,
        source: Source,
    ) -> Result<yamaa_adapters::specification_source::PreparedDocument, Error> {
        match self.prepare_document(source, "retained/spec.yaml".into(), &mut NoParents) {
            Ok(prepared) => Ok(prepared),
            Err(InheritanceError::Entry(error)) => Err(error),
            Err(_) => panic!("standalone dispatch selected inheritance"),
        }
    }
}
#[test]
fn physical_decode_and_source_admission_precede_raw_dispatch_and_zero_semantic_capacity() {
    let mut limits = Limits::default();
    limits.normalization.storage.nodes = 0;
    let schema = schema(limits);
    assert!(matches!(
        schema.dispatch(source("parents: null\n[unterminated")),
        Err(Error::Decode {
            error: DecodeFailure::InvalidYaml { .. },
            ..
        })
    ));
    let mut entry = source("[unterminated");
    entry.bytes.resize(limits.captured_bytes + 1, b'x');
    entry.identity = "x".repeat(limits.identity_bytes + 1);
    assert!(matches!(
        schema.dispatch(entry),
        Err(Error::Limit("captured_bytes"))
    ));
    let mut entry = source("[unterminated");
    entry.identity = "x".repeat(limits.identity_bytes + 1);
    assert!(matches!(
        schema.dispatch(entry),
        Err(Error::Limit("identity_bytes"))
    ));
}
#[test]
fn invalid_schema_default_precedes_authored_version_through_live_dispatch() {
    let schema = CapturedSchema::admit(
        vec![Source {
            identity: "schema.yaml".into(),
            bytes: SCHEMA
                .replace(
                    "type: [str, int], default: DEFAULT",
                    "type: str, default: 7",
                )
                .into_bytes(),
        }],
        0,
        Limits::default(),
    )
    .unwrap();
    let Error::Normalize(NormalizationError::Defaults(defaults)) = schema
        .dispatch(source("schema_version: '99.0'\nwindows: {BAD: broken}\n"))
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(defaults.len(), 1);
    assert_eq!(
        defaults[0].diagnostics,
        vec![yamaa_core::schema::SchemaDiagnostic {
            path: "root_class.domain.default".into(),
            code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
            context: vec![
                (
                    "expected",
                    yamaa_core::schema::SchemaContext::Text("str".into())
                ),
                (
                    "actual",
                    yamaa_core::schema::SchemaContext::ValueKind(
                        yamaa_core::schema::DocumentKind::Integer
                    )
                )
            ],
        }]
    );
}

#[cfg(any(unix, windows))]
mod native {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use yamaa_adapters::{
        file_preparation::{Error as NativeError, FileSpecification},
        file_resources::Resources,
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Study(PathBuf);
    impl Study {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-dispatch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("entry")).unwrap();
            fs::create_dir(path.join("parent")).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn identity(&self, name: &str) -> String {
            let path = self.0.join(name).to_str().unwrap().to_string();
            #[cfg(windows)]
            {
                path.strip_prefix(r"\\?\")
                    .unwrap_or(&path)
                    .replace('\\', "/")
            }
            #[cfg(unix)]
            {
                path
            }
        }
        fn prepare(&self, text: &str) -> Result<FileSpecification, NativeError> {
            fs::write(self.0.join("entry/root.yaml"), text).unwrap();
            FileSpecification::prepare(
                Resources::new(
                    self.0.to_str().unwrap(),
                    self.0.join("entry").to_str().unwrap(),
                    &[],
                )
                .unwrap(),
                "root.yaml",
            )
        }
        fn report(&self, text: &str) -> Value {
            let Err(error) = self.prepare(text) else {
                panic!("native rejection")
            };
            serde_json::from_str(&error.into_message()).unwrap()
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    const VALID:&str="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
    #[test]
    fn native_raw_absent_null_empty_and_scalar_routes_preserve_independent_complete_reports() {
        let study = Study::new();
        let standalone = study.prepare(VALID).unwrap();
        assert!(standalone.run().document().inheritance().is_none());
        assert_eq!(standalone.capture_reads(), 1);
        let inherited = study.prepare(&format!("{VALID}parents: []\n")).unwrap();
        assert!(inherited.run().document().inheritance().is_some());
        assert_eq!(inherited.capture_reads(), 1);
        for parents in ["null", "7"] {
            assert_eq!(
                study.report(&format!("{VALID}parents: {parents}\n")),
                invalid(json!([{
                    "phase":"validation","condition":"invalid_field_type","requirement":"REQ-0287","spec_paths":["parents"],
                    "context":{"expected":"path","actual":if parents=="null" {json!("null")} else {json!("int")},"source":study.identity("entry/root.yaml"),"entry":study.identity("entry/root.yaml")}
                }]))
            );
        }
    }
    #[test]
    fn native_version_missing_and_layer_findings_keep_full_original_identities_and_order() {
        let study = Study::new();
        let root = study.identity("entry/root.yaml");
        assert_eq!(
            study.report(&format!(
                "{}parents: absent.yaml\n",
                VALID.replace("'1.0'", "'99.0'")
            )),
            invalid(json!([{
                "phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0245","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0","source":root,"entry":root}
            }]))
        );
        assert_eq!(
            study.report(&format!("{VALID}parents: absent.yaml\n")),
            invalid(json!([{
                "phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parents"],"context":{"path":"absent.yaml","source":root}
            }]))
        );
        fs::write(
            study.0.join("parent/base.yaml"),
            "schema_version: '1.0'\ndomain: bad-name\n",
        )
        .unwrap();
        assert_eq!(
            study.report(&format!("{VALID}parents: ../parent/base.yaml\n")),
            invalid(json!([{
                "phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","source":study.identity("parent/base.yaml"),"entry":root}
            }]))
        );
    }
    #[test]
    fn native_diamond_retains_first_capture_order_postorder_and_one_snapshot_per_physical_input() {
        let study = Study::new();
        fs::write(study.0.join("parent/common.yaml"), VALID).unwrap();
        fs::write(
            study.0.join("parent/a.yaml"),
            "schema_version: '1.0'\nparents: [common.yaml]\n",
        )
        .unwrap();
        fs::write(
            study.0.join("parent/b.yaml"),
            "schema_version: '1.0'\nparents: [./common.yaml,common.yaml]\n",
        )
        .unwrap();
        let prepared=study.prepare("schema_version: '1.0'\nparents: [../parent/a.yaml, ../parent/b.yaml, ../parent/./a.yaml]\n").unwrap();
        let document = prepared.run().document();
        assert_eq!(
            document
                .parents()
                .iter()
                .map(|p| p.source().identity.as_str())
                .collect::<Vec<_>>(),
            [
                study.identity("parent/a.yaml"),
                study.identity("parent/common.yaml"),
                study.identity("parent/b.yaml")
            ]
        );
        assert_eq!(
            document
                .inheritance()
                .unwrap()
                .layers()
                .iter()
                .map(|l| l.source.identity.as_str())
                .collect::<Vec<_>>(),
            [
                study.identity("parent/common.yaml"),
                study.identity("parent/a.yaml"),
                study.identity("parent/b.yaml"),
                study.identity("entry/root.yaml")
            ]
        );
        assert_eq!(prepared.capture_reads(), 4);
        assert_eq!(prepared.run().source().path, "../parent/never-read.csv");
        assert_eq!(
            yamaa_adapters::specification_check::issues(prepared.run()).unwrap(),
            "[]"
        );
        assert!(!study.0.join("parent/never-read.csv").exists());
        assert!(!study.0.join("parent/never-written.csv").exists());
    }
}
