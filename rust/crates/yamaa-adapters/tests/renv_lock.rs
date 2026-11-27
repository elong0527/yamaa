//! renv lock admission: the reader of `fixtures/renv_lock.json`, whose cases
//! each state a lock's text and the lock it decodes to or the refusal it
//! meets, and the bounds every decoded document shares, built here at their
//! size. `decode_renv` and `project_lock::kind` are the seam: an R host
//! reaches them only behind project activation and the R installation.
use serde_json::{json, Value as Json};
use yamaa_adapters::project_lock::{decode_renv, kind, Error};
use yamaa_adapters::yaml_decode::DecodeFailure;
use yamaa_core::limits::{LimitExceeded, Limits};
use yamaa_core::project_environment::LockKind;

fn limit(limit: &LimitExceeded) -> Json {
    json!({"resource": limit.resource, "limit": limit.limit, "actual": limit.actual})
}

/// What a case states: the decoded lock, or the refusal by its variant.
fn outcome(bytes: &[u8]) -> Json {
    match decode_renv(bytes) {
        Ok(lock) => json!({
            "decoded": {"runtime_version": lock.runtime_version, "packages": lock.packages}
        }),
        Err(Error::Json(_)) => json!({"refused": "json"}),
        Err(Error::Shape(field)) => json!({"refused": {"shape": field}}),
        Err(Error::Limit(exceeded)) => json!({"refused": {"limit": limit(&exceeded)}}),
        Err(Error::Document(DecodeFailure::Limit(exceeded))) => {
            json!({"refused": {"document_limit": limit(&exceeded)}})
        }
        Err(Error::Document(DecodeFailure::InvalidYaml { position, reason })) => json!({
            "refused": {"document": {
                "offset": position.offset,
                "line": position.line,
                "column": position.column,
                "reason": reason,
            }}
        }),
        Err(other) => panic!("no stated refusal is {other:?}"),
    }
}

/// `kind` classifies a lock that decodes as renv, and refuses one that does
/// not as `decode_renv` does.
fn assert_kind(bytes: &[u8], name: &str) {
    match (kind(bytes), decode_renv(bytes)) {
        (Ok(LockKind::Renv), Ok(_)) => {}
        (Err(classified), Err(decoded)) => {
            assert_eq!(format!("{classified:?}"), format!("{decoded:?}"), "{name}");
        }
        (classified, decoded) => panic!("{name}: kind {classified:?}, decode {decoded:?}"),
    }
}

#[test]
fn every_case_decodes_or_refuses_as_stated() {
    let truth: Json = serde_json::from_str(include_str!("fixtures/renv_lock.json")).unwrap();
    let cases = truth["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6, "renv_lock.json case count");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let lock = case["lock"].as_str().unwrap().as_bytes();
        let mut stated = case.as_object().unwrap().clone();
        stated.remove("name");
        stated.remove("lock");
        assert_eq!(outcome(lock), Json::Object(stated), "{name}");
        assert_kind(lock, name);
    }
}

#[test]
fn the_shared_depth_and_size_bounds_hold_at_their_edges() {
    // The seam is the bounds themselves, `Limits::YAML_DEPTH` and
    // `Limits::SPEC_BYTES`, whose edge cases are too large for a fixture.
    let lock = |deep: usize, padding: usize| {
        let nested = format!("{}0{}", "[".repeat(deep), "]".repeat(deep));
        let text = format!(
            r#"{{"R":{{"Version":"4.4.1"}},"Packages":{{}},"Deep":{nested}}}{}"#,
            " ".repeat(padding)
        );
        text.into_bytes()
    };
    let decoded = json!({"decoded": {"runtime_version": "4.4.1", "packages": {}}});
    // The root is at depth 1 and `Deep`'s value at 2, so `deep` arrays put
    // the innermost `0` at depth `deep + 2`.
    let admitted = Limits::YAML_DEPTH - 2;
    let fitted = Limits::SPEC_BYTES - lock(0, 0).len();
    let cases = [
        (
            "the deepest admitted node",
            lock(admitted, 0),
            decoded.clone(),
        ),
        (
            "one node deeper",
            lock(admitted + 1, 0),
            json!({"refused": {"document_limit": {
                "resource": "yaml_depth",
                "limit": Limits::YAML_DEPTH,
                "actual": Limits::YAML_DEPTH + 1,
            }}}),
        ),
        (
            "a lock of the largest admitted size",
            lock(0, fitted),
            decoded,
        ),
        (
            "a lock one byte larger",
            lock(0, fitted + 1),
            json!({"refused": {"limit": {
                "resource": "spec_bytes",
                "limit": Limits::SPEC_BYTES,
                "actual": Limits::SPEC_BYTES + 1,
            }}}),
        ),
    ];
    for (name, bytes, expected) in cases {
        assert_eq!(outcome(&bytes), expected, "{name}");
        assert_kind(&bytes, name);
    }
}
