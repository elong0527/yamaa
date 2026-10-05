# Shared reference binding

The bounded reference service for #1585 owns exact name binding and bare-output
unknown-name, phase and expected-type checks. Optional Python native planning
uses it with an owned catalog compiled once per planning attempt. Installed R
exposes the same core through a stateless batch interface. Python remains the
default and both bindings retain `execution_supported=false`.

## Catalog and rules

`reference_binding::Catalog` owns ordered output and dataset-field identities,
their declared types and reusable name indexes. Binding returns declaration
indices and types, never source values. Bare names bind only output declarations.
A qualified name splits at its first dot and matches the complete remaining
suffix against a stored field. A literal dotted field is allowed; this does not
interpret an ODM item path. Names are case-sensitive, without Unicode
normalization. The immutable Rust catalog contains no borrowed request buffers,
tables, host objects or callbacks.

Bare-output validation first diagnoses an unknown name. If a candidate dataset
contains that name, it returns `unresolvable_name` with the lexicographically
first matching dataset; otherwise it returns `unknown_field`. An existing output
must then be available in the caller's phase, before its exact declared type is
compared with an optional expected type. Integer and float remain distinct for
this check. The host attaches the original reference path, requirement and row
identity; phase failures retain the existing requirement-free diagnostic.

Hosts supply normalized metadata and phase membership. Duplicate catalog names
are rejected. During Python planning, invalid intermediate shadow declarations
can temporarily contribute repeated field names: the adapter supplies the first
visible field, preserving existing binding lookup and the planner's language
diagnostic for the invalid declaration. This does not permit duplicate fields in
the native wire catalog. The Rust snapshot does not make the enclosing Python
plan's dictionaries deeply immutable.

## Installed interfaces

Python and R export `analyze_references` with this strict JSON request:

```json
{"protocol":"reference-analysis/1","catalog":{"outputs":[{"name":"OUT","type":"str"}],"datasets":[{"name":"SRC","fields":[{"name":"X","type":"int"}]}]},"queries":[{"kind":"bind","name":"SRC.X"},{"kind":"validate_output","name":"OUT","expected":"str","available":null,"candidates":[0]}]}
```

It returns:

```json
{"protocol":"reference-analysis/1","outcome":{"status":"complete","results":[{"kind":"binding","binding":{"kind":"dataset","dataset":0,"field":0,"type":"int"}},{"kind":"validation","diagnostic":null}]}}
```

All indices are zero-based declaration indices. Types use the closed vocabulary
`str`, `int`, `float`, `date`, `datetime`. An unresolved binding is `null`.
Validation uses a bare `name`, required `candidates` dataset indices, and optional
`expected` and `available`; omitted or null values disable those respective
checks. An empty available list means no output is available. Validation results
carry `unknown_field`, `unresolvable_name` with a dataset index, `phase_boundary`
with an output index, or `incompatible_input_type` with expected/actual types.

Python planning additionally uses private `_compile_reference_catalog` with a
`reference-catalog/1` request containing only `catalog`. It returns a frozen,
nonconstructible `_ReferenceCatalog` and a JSON status. Its `analyze` method
accepts `reference-queries/1` with `queries` and returns the same analysis outcome
as the batch interface. A compilation limit returns no catalog. No mutable
catalog fields are exposed. R compiles once per batch and retains no external
pointer; it uses the same adapter and core without importing Python.

## Resource and failure contract

Each request admits at most 1,048,576 UTF-8 bytes before JSON decoding. Fixed
catalog policies allow 4,096 outputs, 256 datasets, 65,536 stored fields and
1,048,576 combined name bytes. Individual reference names allow 65,536 UTF-8
bytes. Each batch allows 4,096 queries; available-output and candidate-dataset
lists are bounded by their respective catalog policies, counting written entries.
The core checks catalog count/name budgets before building owned indexes. The
wire decoder's temporary allocations remain bounded by the request byte cap.

Core resource limits return a separate `limit` outcome with resource name and
decimal-string `limit`/`required` counts. The Python planner raises
`NativeReferenceLimitError` with those exact counters. Malformed envelopes,
unknown/duplicate fields, unsupported types/protocols, empty/duplicate catalog
names and invalid context indices are host errors. An invalid later query
rejects the batch without publishing its earlier results. Byte admission errors
also remain host errors. Rejected requests do not mutate a prepared catalog.
Stable error text does not echo caller names or panic payloads. Adapter unwind
containment does not qualify allocation failure, process aborts or host recovery.

The optional frontend captures the compiler before activation or source access.
An older wheel without the service yields `native_reference_binding` unsupported
status at `$` before either effect. Actual catalog compilation occurs after
activation, source schema loading and intermediate catalog construction. Native
compile/query failures propagate without Python binding fallback, automatic
retry or rollback. Explicit retry re-reads sources; a successful activation cache
entry still retains vector qualification. Stateful providers own their recovery.

## Evidence and remaining boundaries

Twenty-nine independent shared cases pin exact bindings, literal dotted names,
Unicode, suggestions, phase/type priority and declaration identities. Rust core,
transport, installed Python batch/prepared interfaces and installed R replay
authored truth. Prepared calls remain valid after their original request is
discarded. Admission tests cover metadata errors, limits and subsequent reuse.

Installed Python planning checks exact diagnostic paths, requirements and
contexts against explicit expectations and the unchanged default planner.
The original ADLB, window, lookup and project-function CSVs run with reference
binding and bare-name fallback disabled. Tests also cover pre-effect admission,
captured service identity, compile/query failures and explicit project retry.
CI runs installed tests against both the wheel and an independently rebuilt
source archive, plus the installed R source package.

This service does not own YAML/schema loading, normalization, derivation
inheritance, phase selection, qualified-reference reachability/grouping/joins,
paired-key type comparisons or typed dataset lowering. Bare-name validation is
connected to row and column output-expression checks; other scope-specific
validation remains in the host. R current-schema compilation, full language and
workflow coverage, numerical policy, performance and release gates remain open.
