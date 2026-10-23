//! The negotiation entry (#3702; the design on #2870, section 3.4): each
//! request of the shared truth's `interface` family, sent through the C
//! entry as a host's client sends it, negotiates to its authored outcome,
//! the table comes back only on acceptance, and a request or outcome too
//! short for its fields is refused in the check order.
use std::mem::offset_of;

use serde_json::{json, Value as Json};
use yamaa_boundary::abi::{self, Str};
use yamaa_boundary::BOUNDARY_VERSION;

use crate::support::{family, outcome};

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

fn texts(value: Option<&Json>) -> Option<Vec<String>> {
    value?
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn strs(texts: &[String]) -> Vec<Str> {
    texts.iter().map(|text| Str::of(text)).collect()
}

/// The texts an outcome points at.
fn listed(items: *const Str, len: usize) -> Vec<String> {
    if len == 0 {
        return Vec::new();
    }
    // SAFETY: an outcome points at static texts.
    unsafe { std::slice::from_raw_parts(items, len) }
        .iter()
        .map(|text| {
            // SAFETY: as above.
            let bytes = unsafe { std::slice::from_raw_parts(text.ptr, text.len) };
            String::from_utf8(bytes.to_vec()).unwrap()
        })
        .collect()
}

/// Negotiate `request` as a host's client sends it: a version that is no
/// positive 32-bit integer as 0, which the entry reads as malformed, and a
/// request whose other fields are not exactly two lists of texts as its
/// header alone, which the entry reads no further than its version. Answer
/// the record the hosts return, whose enabled and missing identifiers the
/// host derives from the supported ones, and whether the table came back.
fn negotiated(request: &Json) -> (Json, bool) {
    let version = request
        .get("version")
        .and_then(Json::as_u64)
        .filter(|version| (1..=u64::from(u32::MAX)).contains(version))
        .map_or(0, |version| version as u32);
    let lists = request
        .as_object()
        .filter(|fields| fields.len() == 3 && fields.contains_key("version"))
        .and_then(|fields| {
            Some((
                texts(fields.get("required"))?,
                texts(fields.get("optional"))?,
            ))
        });
    let (required, optional) = lists.clone().unwrap_or_default();
    let (required_strs, optional_strs) = (strs(&required), strs(&optional));
    let request = abi::Request {
        size: match lists {
            Some(_) => size_of::<abi::Request>(),
            None => offset_of!(abi::Request, required),
        } as u32,
        version,
        required: required_strs.as_ptr(),
        required_len: required_strs.len(),
        optional: optional_strs.as_ptr(),
        optional_len: optional_strs.len(),
    };
    let mut answer = outcome(size_of::<abi::Outcome>());
    // SAFETY: the request and the outcome are valid for the call.
    let table = unsafe { yamaa_engine::negotiate(&request, &mut answer) };
    let supported = listed(answer.supported, answer.supported_len);
    let record = if answer.accepted == 1 {
        let enabled: Vec<&String> = required
            .iter()
            .chain(optional.iter().filter(|name| supported.contains(name)))
            .collect();
        json!({
            "accepted": true,
            "version": answer.requested_version,
            "enabled": enabled,
            "supported": supported,
            "core_version": listed(&answer.core_version, 1)[0],
            "language_versions": listed(answer.language_versions, answer.language_versions_len),
        })
    } else {
        let missing: Vec<&String> = match answer.reason {
            abi::REASON_MISSING_FEATURE => required
                .iter()
                .filter(|name| !supported.contains(name))
                .collect(),
            _ => Vec::new(),
        };
        let reasons = [
            "",
            "version_mismatch",
            "malformed_request",
            "missing_feature",
        ];
        json!({
            "accepted": false,
            "requested_version": (answer.requested_version != 0).then_some(answer.requested_version),
            "supported_version": answer.supported_version,
            "reason": reasons[answer.reason as usize],
            "missing_features": missing,
        })
    };
    // SAFETY: a table the entry returned is the engine's static table.
    let table = unsafe { table.as_ref() }.is_some_and(|table| {
        table.size as usize == size_of::<abi::Engine>() && table.version == BOUNDARY_VERSION
    });
    (record, table)
}

#[test]
fn every_interface_case_negotiates_to_its_authored_outcome_through_the_entry() {
    let interface = family("interface");
    let mut cases = vec![(
        "the generated request".to_owned(),
        resolved(&interface["request"]),
        resolved(&interface["accepted"]),
    )];
    for case in interface["cases"].as_array().unwrap() {
        cases.push((
            case["case"].as_str().unwrap().to_owned(),
            resolved(&case["request"]),
            resolved(&case["outcome"]),
        ));
    }
    for (label, request, expected) in cases {
        let accepted = expected["accepted"] == true;
        assert_eq!(negotiated(&request), (expected, accepted), "{label}");
    }
}

#[test]
fn a_request_or_outcome_too_short_for_its_fields_is_refused() {
    let names: Vec<Str> = ["structured_records", "arrow_c_stream", "plan_reuse"]
        .iter()
        .map(|name| Str::of(name))
        .collect();
    let request = |size: usize| abi::Request {
        size: size as u32,
        version: BOUNDARY_VERSION,
        required: names.as_ptr(),
        required_len: names.len(),
        optional: std::ptr::null(),
        optional_len: 0,
    };
    let fixed = offset_of!(abi::Outcome, reason) + size_of::<u32>();
    // Each case: a request, the outcome's size, and what the entry answers:
    // the table or none, and the fixed part it wrote, or nothing.
    let whole = size_of::<abi::Outcome>();
    let cases: [(&str, Option<abi::Request>, usize, (bool, Option<[u32; 4]>)); 5] = [
        (
            "the generated request",
            Some(request(size_of::<abi::Request>())),
            whole,
            (
                true,
                Some([1, BOUNDARY_VERSION, BOUNDARY_VERSION, abi::REASON_NONE]),
            ),
        ),
        (
            "no request",
            None,
            whole,
            (
                false,
                Some([0, 0, BOUNDARY_VERSION, abi::REASON_MALFORMED_REQUEST]),
            ),
        ),
        (
            "a request that stops inside its version",
            Some(request(offset_of!(abi::Request, version))),
            whole,
            (
                false,
                Some([0, 0, BOUNDARY_VERSION, abi::REASON_MALFORMED_REQUEST]),
            ),
        ),
        (
            "an outcome of only the fixed part",
            Some(request(size_of::<abi::Request>())),
            fixed,
            (
                false,
                Some([
                    0,
                    BOUNDARY_VERSION,
                    BOUNDARY_VERSION,
                    abi::REASON_MALFORMED_REQUEST,
                ]),
            ),
        ),
        (
            "an outcome too short for its fixed part",
            Some(request(size_of::<abi::Request>())),
            fixed - 1,
            (false, None),
        ),
    ];
    for (label, request, size, expected) in cases {
        let mut answer = outcome(size);
        // A mark past the caller's size, which the entry must not write.
        answer.supported_len = usize::MAX;
        answer.reason = u32::MAX;
        let request = request
            .as_ref()
            .map_or(std::ptr::null(), |request| request as *const _);
        // SAFETY: the request and the outcome are valid for the call.
        let table = unsafe { yamaa_engine::negotiate(request, &mut answer) };
        let written = (answer.reason != u32::MAX).then_some([
            answer.accepted,
            answer.requested_version,
            answer.supported_version,
            answer.reason,
        ]);
        let untouched = size == whole || answer.supported_len == usize::MAX;
        assert_eq!(
            (!table.is_null(), written, untouched),
            (expected.0, expected.1, true),
            "{label}"
        );
    }
}
