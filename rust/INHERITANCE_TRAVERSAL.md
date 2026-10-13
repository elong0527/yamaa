# Typed inheritance traversal

The `inheritance/1` callback protocol and R `traverse_inheritance` export are
retired under #1988. Both installed hosts use typed native inheritance capture;
JSON request/reply envelopes no longer cross an inheritance source callback.
The obsolete R wrapper, native registration and transport implementation are
removed. The function/dataset probe protocols remain owned by #1990.

## Ownership and ordering

`yamaa-engine::inheritance` owns original-layer admission, parent path/version
checks, left-to-right postorder, canonical deduplication and cycles, and cumulative
logical budgets. `inheritance_capture` and `inheritance_preparation` retain typed
source captures and declaring-file provenance. Adapters decode held YAML and
resolve native file resources; they grant no semantic fallback.

The entry is captured once. Parents are requested only after the preceding layer
and its version pass. A canonical source is read once, including diamond graphs;
cycles fail before a repeated read. A failure stops later parent and sibling
requests. Relative resource declarations remain anchored to their original file.

`inheritance_outcome::traversal_outcome` projects typed engine errors to a portable
core diagnostic or returns the original owned error. Host errors retain their
actual allocation. The adapter serializes findings through the common
`spec_paths` and JSON-text context representation. Limits and unclassified failures
remain explicit refusals; no partial successful traversal is returned.

## Retained qualification

The frozen `inheritance_traversal.tsv` and `inheritance_sources.tsv` vectors remain
inputs to `stage_specification_inputs.py` and both installed original-specification
suites. Those typed consumers retain graph results and source-effect expectations.
`inheritance_preparation.tsv` additionally pins complete original-preparation
reports and capture counts in the Rust capture suite.

The engine inheritance suite covers postorder/aliases, canonical cycles, early
layer/version failures, missing and opaque sources, cumulative limits, recovery
and complete portable cause/provenance projection. Its owned-error test exercises
the new typed outcome transfer. The installed Python typed port suite retains
host exception identity, interruption, reentrancy and thread-isolation checks.
Public Python/R conformance retains the inherited specification benchmarks.

The removed transport suite's JSON framing, malformed reply and cumulative JSON
reply-byte checks apply only to the retired protocol. The removed R suite's
custom JSON dispatcher checks have no retained public callback consumer. Shared
graph semantics and supported public behavior remain qualified by the typed
suites above; component evidence does not establish complete release approval.
