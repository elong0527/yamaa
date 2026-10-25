//! The version and feature handshake (`rust/interface/README.md`): each
//! request of the shared truth's `interface` family negotiates to its
//! authored outcome, the version is compared before anything else is read,
//! R's vectors read as their element and as sequences, and a binding's gate
//! opens on the first acceptance only.
use serde_json::{json, Value as Json};
use yamaa_boundary::{
    negotiate, Accepted, Closed, EngineFacts, Gate, Outcome, Reason, Refusal, Value,
    BOUNDARY_VERSION,
};

use crate::support::{family, texts};

/// The engine of a shipped build, which compiles every format, as the
/// truth's `info` family gives it, with this crate's release.
fn shipped() -> EngineFacts {
    let info = family("info");
    EngineFacts::new(
        &texts(info, "features"),
        env!("CARGO_PKG_VERSION"),
        &texts(info, "language_versions"),
    )
}

/// The truth with its placeholders filled: `{version}` is the boundary
/// version, `{next_version}` the one after it and `{core_version}` the
/// engine's release.
fn resolved(value: &Json) -> Json {
    match value {
        Json::String(text) if text == "{version}" => json!(BOUNDARY_VERSION),
        Json::String(text) if text == "{next_version}" => json!(BOUNDARY_VERSION + 1),
        Json::String(text) if text == "{core_version}" => json!(env!("CARGO_PKG_VERSION")),
        Json::Array(items) => Json::Array(items.iter().map(resolved).collect()),
        Json::Object(fields) => Json::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), resolved(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A request as a host builds it from JSON: an object is a mapping, an array
/// a list, a whole number within 64 bits an integer, and a boolean, a
/// fraction or null no field of a request.
fn request(value: &Json) -> Value {
    match value {
        Json::Number(number) => number.as_i64().map_or(Value::Other, Value::Integer),
        Json::String(text) => Value::Text(text.clone()),
        Json::Array(items) => Value::List(items.iter().map(request).collect()),
        Json::Object(fields) => Value::Map(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), request(item)))
                .collect(),
        ),
        Json::Bool(_) | Json::Null => Value::Other,
    }
}

/// An outcome as the hosts return it: the accepted record or the envelope.
fn record(outcome: &Outcome) -> Json {
    match outcome {
        Outcome::Accepted(accepted) => json!({
            "accepted": true,
            "version": accepted.version,
            "enabled": accepted.enabled,
            "supported": accepted.supported,
            "core_version": accepted.core_version,
            "language_versions": accepted.language_versions,
        }),
        Outcome::Refused(refusal) => json!({
            "accepted": false,
            "requested_version": refusal.requested_version,
            "supported_version": refusal.supported_version,
            "reason": refusal.reason.as_str(),
            "missing_features": refusal.missing_features,
        }),
    }
}

fn text(identifier: &str) -> Value {
    Value::Text(identifier.to_owned())
}

/// The three required identifiers of the generated request, as `items`.
fn required(items: fn(Vec<Value>) -> Value) -> Value {
    items(
        ["structured_records", "arrow_c_stream", "plan_reuse"]
            .map(text)
            .to_vec(),
    )
}

fn refused(requested_version: Option<u32>, reason: Reason) -> Outcome {
    Outcome::Refused(Refusal {
        requested_version,
        supported_version: Some(BOUNDARY_VERSION),
        reason,
        missing_features: Vec::new(),
    })
}

#[test]
fn every_interface_case_negotiates_to_its_authored_outcome() {
    let interface = family("interface");
    let engine = shipped();
    let generated = [(
        "the generated request",
        &interface["request"],
        &interface["accepted"],
    )];
    let crafted = interface["cases"].as_array().unwrap().iter().map(|case| {
        (
            case["case"].as_str().unwrap(),
            &case["request"],
            &case["outcome"],
        )
    });
    for (case, value, outcome) in generated.into_iter().chain(crafted) {
        let answered = negotiate(&request(&resolved(value)), &engine);
        assert_eq!(record(&answered), resolved(outcome), "{case}");
    }
}

/// A request for another version is refused as a mismatch whatever the rest
/// holds: lists that hold no identifier, a list given twice and an unknown
/// key, none of which JSON can write together.
#[test]
fn the_version_is_compared_before_any_list_is_read() {
    let next = BOUNDARY_VERSION + 1;
    let request = Value::Map(vec![
        ("required".into(), Value::Other),
        ("version".into(), Value::Integer(i64::from(next))),
        ("required".into(), Value::List(vec![Value::Other])),
        ("unknown".into(), Value::Vector(vec![Value::Other])),
    ]);
    assert_eq!(
        negotiate(&request, &shipped()),
        refused(Some(next), Reason::VersionMismatch)
    );
}

/// Requests that only R builds: an atomic vector reads as its one element
/// where a scalar belongs and as a sequence where a list belongs, and a list
/// with names may repeat a name.
#[test]
fn an_r_request_negotiates_by_its_vectors_and_names() {
    let version = Value::Integer(i64::from(BOUNDARY_VERSION));
    let one = |value: Value| Value::Vector(vec![value]);
    let empty = Value::Vector(Vec::new());
    let field = |key: &str, value: Value| (key.to_owned(), value);
    // The accepted record of the truth, with the identifiers each case enables.
    let accepted = |enabled: &[&str]| {
        Outcome::Accepted(Accepted {
            version: BOUNDARY_VERSION,
            enabled: enabled
                .iter()
                .map(|&identifier| identifier.into())
                .collect(),
            supported: texts(&family("interface")["accepted"], "supported"),
            core_version: env!("CARGO_PKG_VERSION").into(),
            language_versions: texts(family("info"), "language_versions"),
        })
    };
    let cases = [
        (
            "a version and lists as atomic vectors",
            Value::Map(vec![
                field("version", one(version.clone())),
                field("required", required(Value::Vector)),
                field("optional", empty.clone()),
            ]),
            accepted(&["structured_records", "arrow_c_stream", "plan_reuse"]),
        ),
        (
            "a list of one-element vectors",
            Value::Map(vec![
                field("version", version.clone()),
                field(
                    "required",
                    Value::List(vec![one(text("plan_reuse")), one(text("arrow_c_stream"))]),
                ),
                field("optional", Value::List(Vec::new())),
            ]),
            accepted(&["plan_reuse", "arrow_c_stream"]),
        ),
        (
            "a one-element vector of identifiers is a list of one",
            Value::Map(vec![
                field("version", version.clone()),
                field("required", one(text("structured_records"))),
                field("optional", one(text("format_yaml"))),
            ]),
            accepted(&["structured_records", "format_yaml"]),
        ),
        (
            "a version vector of two elements is malformed",
            Value::Map(vec![
                field(
                    "version",
                    Value::Vector(vec![version.clone(), version.clone()]),
                ),
                field("required", required(Value::Vector)),
                field("optional", empty.clone()),
            ]),
            refused(None, Reason::MalformedRequest),
        ),
        (
            "a version given twice is malformed",
            Value::Map(vec![
                field("version", version.clone()),
                field("version", version.clone()),
                field("required", required(Value::Vector)),
                field("optional", empty.clone()),
            ]),
            refused(None, Reason::MalformedRequest),
        ),
        (
            "a list given twice is malformed",
            Value::Map(vec![
                field("version", version.clone()),
                field("required", required(Value::Vector)),
                field("required", required(Value::Vector)),
            ]),
            refused(Some(BOUNDARY_VERSION), Reason::MalformedRequest),
        ),
    ];
    for (case, request, outcome) in cases {
        assert_eq!(negotiate(&request, &shipped()), outcome, "{case}");
    }
}

/// The gate refuses until a negotiation is accepted, and a refused
/// negotiation, before or after, changes nothing.
#[test]
fn the_first_acceptance_opens_the_gate_and_nothing_closes_it() {
    let gate = Gate::new();
    let engine = shipped();
    let interface = family("interface");
    let generated = request(&resolved(&interface["request"]));
    let mismatch = Value::Map(vec![(
        "version".into(),
        Value::Integer(i64::from(BOUNDARY_VERSION) + 1),
    )]);
    let mut steps = vec![("before any negotiation", gate.pass())];
    for (step, value) in [
        ("after a refusal", &mismatch),
        ("after an acceptance", &generated),
        ("after a later refusal", &mismatch),
        ("after a later acceptance", &generated),
    ] {
        gate.negotiate(value, &engine);
        steps.push((step, gate.pass()));
    }
    assert_eq!(
        steps,
        [
            ("before any negotiation", Err(Closed)),
            ("after a refusal", Err(Closed)),
            ("after an acceptance", Ok(())),
            ("after a later refusal", Ok(())),
            ("after a later acceptance", Ok(())),
        ]
    );
}

#[test]
fn a_closed_gate_refuses_with_the_message_of_the_truth() {
    assert_eq!(
        Closed.to_string(),
        family("interface")["gate_message"].as_str().unwrap()
    );
}
