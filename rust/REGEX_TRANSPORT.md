# Installed portable regex service

`yamaa_native.evaluate_regex` and `yamaanative::evaluate_regex` expose the same
bounded `regex/1` service. They compile and match through `yamaa-core`, without
Python `re`, R's regex implementation or fallback. This optional service does
not activate dataset execution or change Python's default backend.

## Request and outcome

The request is an owned JSON string with exactly `protocol`, `pattern` and
`operation`. The protocol is `regex/1` and the pattern is a string. An operation
is one of these closed objects:

```json
{"kind":"compile"}
{"kind":"search","subject":"ab"}
{"kind":"full_match","subject":"ab"}
```

Compile-only requests reject a subject. Search and full-match requests require
a string subject. Missing/null subjects, flags, unknown or duplicate keys,
invalid UTF-8/scalar encodings and malformed JSON are transport defects, not
regex outcomes. Both adapters return normally from Rust before exposing a host
error. Python uses `ValueError` for transport defects and `RuntimeError` for
contained internal failures; wrong host argument types use `TypeError`. The R
facade requires an unclassed, attribute-free, nonmissing scalar character value,
validates its raw UTF-8 boundary and raises errors after the native call returns.

For example, this request searches for a capture whose optional part is absent:

```json
{"protocol":"regex/1","pattern":"^(a)?()b$","operation":{"kind":"search","subject":"b"}}
```

Its response is:

```json
{"protocol":"regex/1","contract_version":"2.0.0","outcome":{"status":"matched","group_count":2,"groups":["b",null,""]}}
```

Every response identifies the service and portable grammar versions. Outcomes
are `compiled` (capture count only), `matched` (count plus groups zero through
the last numbered group), `no_match`, `invalid`, `unsupported` or
`resource_limit`. Empty captures are empty strings, unentered groups are JSON
null, and no match has its own status. Full-match evaluation preserves alternate
paths that fail the final anchor; it is not a search followed by a length check.
Strings retain exact Unicode scalars without normalization. Response JSON is
owned by the caller; later calls cannot replace its capture storage.

An invalid pattern reports `invalid_regex`, REQ-0827, a reason and zero-based
UTF-8 byte/Unicode scalar positions. The unsupported outcome is reserved for
explicit compiler capability refusals. Resource outcomes report phase
(`compile`, `match` or `response`), resource and limit. They are never reported
as invalid syntax or ordinary no-match, and no partial captures are returned.

The whole transport envelope is decoded and validated first. Pattern compilation
then precedes subject matching and its budgets. The decoded subject is already
owned at this point; this API does not claim to defer JSON allocation. A caller
that must validate before loading data can issue the subject-free compile
operation. Future predicate integration must preserve REQ-1244's distinct
`invalid_predicate` mapping at parse time; this service does not choose that
consumer-specific condition or any lifecycle handler.

## Resource policy

Requests and serialized responses each have an independent 1,048,576-byte limit.
The request limit is checked before JSON decoding. Core compilation and matching
use the default independent budgets documented in
[the regex assessment](REGEX_ASSESSMENT.md); fitting a transport envelope does
not promise that pattern admission, subject indexing or matching will fit them.
There are no caller-supplied policy overrides in this protocol.

Capture slices serialize directly into a bounded writer, without first cloning
their text into a JSON value tree. The writer charges escaped UTF-8 bytes before
each append, so overlapping captures and control-character escaping cannot
create an unbounded response. Exhaustion discards partial output and returns a
small `response_bytes` refusal. All budgets restart on a fresh call. These
logical policies do not bound pre-call host allocations, allocator capacity,
total process memory or concurrent calls.

## Qualification and remaining work

The shared `regex_transport.tsv` contains 38 authored request/outcome cases,
replayed by the adapter and both installed hosts. Additional tests cover strict
decoding (including an unused compile-only subject), Unicode/NUL transport,
compile-before-match failure order, response expansion, limits and retry. Python
tests forbid `re.compile` while replaying the shared truth. Native CI runs the
installed suite from direct and source-rebuilt Python 3.14 wheels on all three
hosts, the regex suite from each direct wheel on Python 3.12, and installed R
source packages on both R hosts. The existing 2,101 core
observations and reference discrepancy report remain separate evidence.

This is a regex service, not a current-schema specification compiler. Predicate
`str_contains`, schema-pattern descriptors, verification `matches`, text
extraction, lifecycle integration and full grammar/release qualification remain
gated. The known Python reference discrepancies are not tolerances or rewritten
expectations. Python remains default and `execution_supported=false`.
