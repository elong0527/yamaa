//! Shipped schema identity and raw preparation without host schema interpretation.
use yamaa_adapters::{
    shipped_schema,
    specification_source::{Error, InheritanceError, InheritancePort, Source},
};
use yamaa_engine::inheritance::{Source as Identity, SourceError};

struct NoParents;
impl InheritancePort for NoParents {
    type Error = ();
    fn canonicalize(&mut self, _: &str, _: &str) -> Result<Identity, SourceError<()>> {
        panic!("entry rejection or standalone preparation must not resolve a parent")
    }
    fn capture(&mut self, _: &Identity, _: usize) -> Result<Vec<u8>, SourceError<()>> {
        panic!("entry rejection or standalone preparation must not read a parent")
    }
    fn rebase(&mut self, _: &Identity, _: &Identity, _: &str, _: usize) -> Result<String, ()> {
        panic!("entry rejection or standalone preparation must not rebase a parent")
    }
}
fn source(text: &str) -> Source {
    Source {
        identity: "arbitrary/location/domain.yaml".into(),
        bytes: text.as_bytes().to_vec(),
    }
}
const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n";

#[test]
fn shipped_closure_retains_every_authoritative_byte_and_admits_without_a_directory() {
    let schema = shipped_schema::capture().unwrap();
    assert_eq!(schema.structure().version(), "1.0");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../yaml");
    let mut expected = std::fs::read_dir(&root)
        .unwrap()
        .map(|p| p.unwrap().file_name().into_string().unwrap())
        .filter(|p| p == "schema.yaml" || p.starts_with("schema_") && p.ends_with(".yaml"))
        .filter(|p| p != "schema_define.yaml" && p != "schema_environment.yaml")
        .collect::<Vec<_>>();
    expected.sort();
    let mut actual = schema
        .sources()
        .iter()
        .map(|s| s.identity.clone())
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
    for module in schema.sources() {
        assert_eq!(
            module.bytes,
            std::fs::read(root.join(&module.identity)).unwrap()
        );
    }
    let prepared =
        shipped_schema::prepare(source(VALID), "domain.yaml".into(), &mut NoParents).unwrap();
    assert!(prepared.inheritance().is_none());
    assert_eq!(prepared.source().bytes, VALID.as_bytes());
    assert!(prepared.parents().is_empty());
}

#[test]
fn unsupported_or_missing_version_is_rejected_before_any_parent_authority() {
    for text in [
        VALID.replace("'1.0'", "'99.0'"),
        VALID.replace("schema_version: '1.0'\n", ""),
    ] {
        let result = shipped_schema::prepare(source(&text), "domain.yaml".into(), &mut NoParents);
        let Err(InheritanceError::Entry(Error::Findings(captured))) = result else {
            panic!("expected shared schema version finding");
        };
        assert_eq!(captured.findings().len(), 1);
        assert_eq!(captured.findings()[0].condition, "schema_version_mismatch");
    }
    let malformed =
        shipped_schema::prepare(source("[unclosed"), "domain.yaml".into(), &mut NoParents);
    assert!(matches!(
        malformed,
        Err(InheritanceError::Entry(Error::Decode { .. }))
    ));
    // Presence selects inheritance even for an invalid null field; no fallback.
    let inherited = shipped_schema::prepare(
        source(&(VALID.to_owned() + "parents: null\n")),
        "domain.yaml".into(),
        &mut NoParents,
    );
    assert!(matches!(
        inherited,
        Err(InheritanceError::Preparation { .. })
    ));
}

#[test]
fn standalone_findings_retain_source_and_resolve_context_without_host_interpretation() {
    use serde_json::{json, Value};
    use yamaa_adapters::specification_diagnostics::capture_failure;
    let cases = [
        (
            VALID.replace("'1.0'", "'99.0'"),
            json!({"phase":"validation","condition":"schema_version_mismatch","requirement":null,"spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0"}}),
        ),
        (
            VALID.replace("schema_version: '1.0'\n", ""),
            json!({"phase":"validation","condition":"schema_version_mismatch","requirement":null,"spec_paths":["schema_version"],"context":{"expected":"1.0","actual":null}}),
        ),
        (
            VALID.replace("domain: TEST", "domain: bad-name"),
            json!({"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$"}}),
        ),
        (
            VALID.replace("type: int", "type: unknown"),
            json!({"phase":"validation","condition":"value_not_permitted","requirement":"REQ-0012","spec_paths":["columns.ID.type"],"context":{"value":"unknown","permitted":["str","int","float","date","datetime"]}}),
        ),
        (
            VALID.replace("path: output.csv", "path: ''"),
            json!({"phase":"validation","condition":"minimum_length","requirement":"REQ-0287","spec_paths":["output.path"],"context":{"minimum":1}}),
        ),
    ];
    for (text, expected) in cases {
        let schema = shipped_schema::capture().unwrap();
        let error = schema.prepare_standalone(source(&text)).unwrap_err();
        drop(schema);
        let Error::Findings(captured) = &error else {
            panic!("expected captured findings");
        };
        assert_eq!(captured.source().bytes, text.as_bytes());
        assert!(captured.document().is_some());
        assert_eq!(captured.schema().structure().version(), "1.0");
        let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
        assert_eq!(
            actual,
            json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[expected]}})
        );
    }
    let integer = "922337203685477580812345678901234567890";
    let error = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(source(&VALID.replace("'1.0'", integer)))
        .unwrap_err();
    let encoded = capture_failure(&error);
    assert!(encoded.contains(&format!("\"actual\":{integer}")));
    let actual: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        actual["outcome"]["diagnostics"][0]["context"]["actual"].to_string(),
        integer
    );
}

#[test]
fn window_expansion_findings_use_the_retained_normalized_arena() {
    use serde_json::{json, Value};
    use yamaa_adapters::specification_diagnostics::capture_failure;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks/negative-unknown-window");
    let raw = std::fs::read(root.join("spec.yaml")).unwrap();
    let error = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: raw.clone(),
        })
        .unwrap_err();
    let Error::Findings(captured) = &error else {
        panic!("expected window findings");
    };
    assert_eq!(captured.source().bytes, raw);
    let document = captured.document().unwrap();
    // Shorthand expansion changes this pass input from the authored arena.
    let decoded = yamaa_adapters::yaml_decode::decode_yaml(&raw, Default::default()).unwrap();
    assert_ne!(document, &decoded.document);
    let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
    // Independent expected/error.yaml is unchanged.
    assert_eq!(
        actual["outcome"],
        json!({"status":"invalid","diagnostics":[{"phase":"validation","condition":"unknown_window","spec_paths":["columns.VISITSEQ.derivation.row_number.window"],"requirement":"REQ-1253","context":{"window":"VISITS_ORDER"}}]})
    );
}

#[test]
fn diagnostic_expansion_limits_do_not_masquerade_as_language_findings() {
    use serde_json::{json, Value};
    use yamaa_adapters::specification_diagnostics::capture_failure;
    // A source string can fit input limits but exceed the conservative escaped-JSON budget.
    let text = format!("schema_version: '{}'\n", "x".repeat(2_800_000));
    let error = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(source(&text))
        .unwrap_err();
    assert!(matches!(&error, Error::Findings(_)));
    let actual: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
    assert_eq!(
        actual["outcome"],
        json!({"status":"rejected","stage":"capture","code":"diagnostic_context_limit"})
    );
}

#[test]
fn source_failures_preserve_independent_decoder_findings_before_any_host_effect() {
    use serde_json::{json, Value};
    use yamaa_adapters::specification_diagnostics::inheritance_failure;
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
            finding["requirement"] = Value::Null;
            if finding["condition"] == "non_ascii_source" {
                finding["context"]["path"] = json!("source.yaml");
            }
        }
        let bytes = fields[1]
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        let error = shipped_schema::prepare(
            Source {
                identity: "source.yaml".into(),
                bytes,
            },
            "source.yaml".into(),
            &mut NoParents,
        )
        .unwrap_err();
        let actual: Value = serde_json::from_str(&inheritance_failure(error).unwrap()).unwrap();
        assert_eq!(actual, expected, "{}", fields[0]);
        count += 1;
    }
    assert_eq!(count, 8);
}

struct ParentSource {
    raw: Vec<u8>,
    reads: usize,
}
impl InheritancePort for ParentSource {
    type Error = ();
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<()>> {
        assert_eq!((declaring, written), ("entry.yaml", "parent.yaml"));
        Ok(Identity {
            identity: "parent.yaml".into(),
            display_path: "parent.yaml".into(),
        })
    }
    fn capture(&mut self, _: &Identity, maximum: usize) -> Result<Vec<u8>, SourceError<()>> {
        assert!(self.raw.len() <= maximum);
        self.reads += 1;
        Ok(self.raw.clone())
    }
    fn rebase(
        &mut self,
        _: &Identity,
        _: &Identity,
        written: &str,
        _: usize,
    ) -> Result<String, ()> {
        Ok(written.into())
    }
}
fn inherited_error(entry: &str, parent: &str) -> InheritanceError<()> {
    let mut port = ParentSource {
        raw: parent.as_bytes().to_vec(),
        reads: 0,
    };
    let error = shipped_schema::prepare(
        Source {
            identity: "entry.yaml".into(),
            bytes: entry.as_bytes().to_vec(),
        },
        "entry.yaml".into(),
        &mut port,
    )
    .unwrap_err();
    if port.reads != 1 {
        panic!(
            "unexpected early failure: {}",
            yamaa_adapters::specification_diagnostics::inheritance_failure(error).unwrap()
        );
    }
    error
}
#[test]
fn inherited_layer_and_composed_findings_resolve_without_exposing_context_arenas() {
    use serde_json::{json, Value};
    use yamaa_adapters::specification_diagnostics::inheritance_failure;
    let entry = format!("{VALID}parents: parent.yaml\n");
    let invalid_parent = "schema_version: '1.0'\ndomain: bad-name\n";
    let outcome: Value = serde_json::from_str(
        &inheritance_failure(inherited_error(&entry, invalid_parent)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"invalid","diagnostics":[{"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","source":"parent.yaml","entry":"entry.yaml"}}]})
    );
    let empty_parent = "schema_version: '1.0'\n";
    let missing = inherited_error(&entry.replace("domain: TEST\n", ""), empty_parent);
    let InheritanceError::Preparation { error, .. } = &missing else {
        panic!("expected lifecycle failure");
    };
    let yamaa_engine::inheritance_preparation::Error::Normalize(context) = error.as_ref() else {
        panic!("expected normalization failure");
    };
    assert!(context.input.is_some());
    let outcome: Value = serde_json::from_str(&inheritance_failure(missing).unwrap()).unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"invalid","diagnostics":[{"phase":"validation","condition":"missing_required_field","requirement":null,"spec_paths":["domain"],"context":{"field":"domain","class":"root_class"}}]})
    );
    let unknown = inherited_error(
        &entry.replace("source: SRC.ID", "compute: {expr: UNKNOWN}"),
        empty_parent,
    );
    let outcome: Value = serde_json::from_str(&inheritance_failure(unknown).unwrap()).unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"invalid","diagnostics":[{"phase":"validation","condition":"unknown_reference","requirement":"REQ-0070","spec_paths":["columns.ID.derivation"],"context":{"column":"ID","dependency":"UNKNOWN"}}]})
    );
}
