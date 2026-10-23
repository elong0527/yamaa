//! The negotiation entry (#3702; the design on #2870, section 3.4): it
//! answers each request in the boundary's check order, reads another
//! version's request no further than its header, writes no field past the
//! caller's outcome, and returns the table only on acceptance. Each outcome
//! is derived from the check order of `yamaa-boundary` and the fixed part of
//! the outcome that `abi::Outcome` documents.
use std::mem::offset_of;

use yamaa_boundary::abi::{self, Str};
use yamaa_boundary::BOUNDARY_VERSION;

use crate::support::{family, outcome, request};

/// What one negotiation answered: whether the table came back, the fixed
/// part if written, and the supported identifiers if written.
type Negotiated = (bool, Option<[u32; 4]>, Vec<String>);

/// Negotiate `request` into an outcome of `size` bytes, checking that no
/// field past the caller's size is written.
fn negotiated(request: Option<&abi::Request>, size: usize) -> Negotiated {
    let mut answer = outcome(size);
    // Marks past a short caller's size, which the entry must not write.
    (answer.reason, answer.supported_len) = (u32::MAX, usize::MAX);
    let request = request.map_or(std::ptr::null(), |request| request as *const _);
    // SAFETY: the request and the outcome are valid for the call.
    let table = unsafe { yamaa_engine::negotiate(request, &mut answer) };
    let fixed = (answer.reason != u32::MAX).then_some([
        answer.accepted,
        answer.requested_version,
        answer.supported_version,
        answer.reason,
    ]);
    let whole = size >= size_of::<abi::Outcome>();
    assert!(
        whole || answer.supported_len == usize::MAX,
        "a write past the outcome's size"
    );
    let supported = match whole && answer.supported_len != usize::MAX {
        // SAFETY: an outcome points at static texts.
        true => unsafe { std::slice::from_raw_parts(answer.supported, answer.supported_len) }
            .iter()
            // SAFETY: as above.
            .map(|text| unsafe { std::slice::from_raw_parts(text.ptr, text.len) })
            .map(|bytes| String::from_utf8(bytes.to_vec()).unwrap())
            .collect(),
        false => Vec::new(),
    };
    // SAFETY: a table the entry returns is the engine's static table.
    let table = unsafe { table.as_ref() }.is_some_and(|table| {
        (table.size as usize, table.version) == (size_of::<abi::Engine>(), BOUNDARY_VERSION)
    });
    (table, fixed, supported)
}

#[test]
fn each_request_negotiates_in_the_check_order_through_the_entry() {
    let names = |names: &[&'static str]| names.iter().map(|name| Str::of(name)).collect::<Vec<_>>();
    let required = names(&["structured_records", "arrow_c_stream", "plan_reuse"]);
    let (outside, unknown) = (
        names(&["Format_YAML"]),
        names(&["structured_records", "batch_stream"]),
    );
    let whole = size_of::<abi::Outcome>();
    let fixed = offset_of!(abi::Outcome, reason) + size_of::<u32>();
    let sized = |size: usize, version: u32, lists: &[Str]| abi::Request {
        size: size as u32,
        version,
        ..request(lists, &[])
    };
    let full = size_of::<abi::Request>();
    // A next version's lists that this version could not read, and does not.
    let next = abi::Request {
        required: std::ptr::null(),
        required_len: 7,
        ..sized(full, BOUNDARY_VERSION + 1, &[])
    };
    let supported: Vec<String> = family("interface")["accepted"]["supported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect();
    let (version, malformed) = (BOUNDARY_VERSION, abi::REASON_MALFORMED_REQUEST);
    let cases: [(&str, Option<abi::Request>, usize, Negotiated); 9] = [
        (
            "the required identifiers",
            Some(request(&required, &[])),
            whole,
            (
                true,
                Some([1, version, version, abi::REASON_NONE]),
                supported.clone(),
            ),
        ),
        (
            "no request",
            None,
            whole,
            (false, Some([0, 0, version, malformed]), Vec::new()),
        ),
        (
            "a request that stops inside its version",
            Some(sized(offset_of!(abi::Request, version), version, &required)),
            whole,
            (false, Some([0, 0, version, malformed]), Vec::new()),
        ),
        (
            "a version of zero",
            Some(sized(full, 0, &required)),
            whole,
            (false, Some([0, 0, version, malformed]), Vec::new()),
        ),
        (
            "the next version, read no further than its header",
            Some(next),
            whole,
            (
                false,
                Some([0, version + 1, version, abi::REASON_VERSION_MISMATCH]),
                Vec::new(),
            ),
        ),
        (
            "this version without its lists",
            Some(sized(
                offset_of!(abi::Request, required),
                version,
                &required,
            )),
            whole,
            (false, Some([0, version, version, malformed]), Vec::new()),
        ),
        (
            "an identifier outside the grammar",
            Some(request(&outside, &[])),
            whole,
            (false, Some([0, version, version, malformed]), Vec::new()),
        ),
        // The host names the missing identifiers: those of `required` that
        // the supported ones lack, in request order.
        (
            "an unknown required identifier",
            Some(request(&unknown, &[])),
            whole,
            (
                false,
                Some([0, version, version, abi::REASON_MISSING_FEATURE]),
                supported,
            ),
        ),
        (
            "an outcome of only its fixed part",
            Some(request(&required, &[])),
            fixed,
            (false, Some([0, version, version, malformed]), Vec::new()),
        ),
    ];
    for (label, request, size, expected) in cases {
        assert_eq!(negotiated(request.as_ref(), size), expected, "{label}");
    }
    assert_eq!(
        negotiated(Some(&request(&required, &[])), fixed - 1),
        (false, None, Vec::new()),
        "an outcome too short for its fixed part"
    );
}
