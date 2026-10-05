# Shared reference binding

The bounded reference service for #1585 owns exact name binding, bare-output
unknown-name/phase/type checks, direct qualified-field scope/grouping, and
intermediate field visibility/donor-scope checks. Optional Python native planning
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

## Direct qualified-field validation

Both bindings export `reference_capabilities()`, returning the
`reference-analysis/1` protocol with `binding`, `output_validation` and
`qualified_validation` and `intermediate_validation` features. Existing query shapes remain unchanged. The
optional Python frontend requires the qualified feature before activation or
source access; older packages return `native_qualified_reference_validation`
unsupported status at `$`. Capability discovery grants no data or callback
authority. The already captured compiler creates the actual catalog after source
metadata is available.

Batch and prepared interfaces accept this additional query, using the catalog
from the installed-interface example above:

```json
{"kind":"validate_qualified","name":"SRC.X","expected":"str","scope":{"drivers":["SRC"],"current_driver":false,"reach":"scalar","joined":false,"phase":{"kind":"row","group_by":null}}}
```

The result is `qualified_validation` with ordered `diagnostics`, here
`[{"kind":"incompatible_input_type","expected":"str","actual":"int"}]`.
The query must name a qualified stored field. `expected` may be omitted or null.
`scope` requires drivers, current_driver, reach, joined and phase. Reach is one
of `scalar`, `record`, `relation` or `declared`. A row phase has optional/null
`group_by` for an ungrouped row, or a list of complete qualified group names;
an empty list is distinct from null. A column phase requires `groups`, the group
name lists of the bound dataset's grouped templates. These are normalized compiler
facts, not user assertions that permit source access. The service does not select
drivers, infer joins, validate the group declarations themselves or select phases.

Shared rules retain the reference order: current-driver mismatch, then field
existence, then the already-failed implicit-join suppression rule, then row-phase
or grouping checks, then strict expected type. Driver/existence findings stop
further checks. Grouping or row-phase findings retain a later type finding.
Declared/record reads and planned joins preserve row-phase exemptions; a direct
foreign scalar source with an unresolved implicit join does not gain a second
phase failure. Column scalar reads must be a group key in every supplied grouped
template. No new implicit int/float conversion is introduced.

Finding kinds are `driver_mismatch`, `unknown_field`, `row_phase`, `row_group`,
`column_group`, and `incompatible_input_type`. Hosts attach the authored reference
and row names/path. Unknown fields use REQ-0103; row/column grouping uses
REQ-0067/0107; row-phase findings have no requirement. Driver/type findings keep
the originating operation's requirement. Multiple findings retain that order.

Scope admission allows 256 written driver names, 4,096 groups, 65,536 total group
name entries and 1,048,576 combined UTF-8 context bytes per query. Duplicate entries
count toward policy limits. Name and request/query budgets above also apply.
Malformed scope shapes, unknown reaches/phases and bare qualified-query names are
transport errors; scope resource failures remain separate limit outcomes. Context
is consumed per query without a mutable scope cache. Native findings do not
authorize execution of an otherwise invalid or unsupported plan.

The optional frontend captures the compiler before activation or source access.
An older wheel without the service yields `native_reference_binding` unsupported
status at `$` before either effect. Actual catalog compilation occurs after
activation, source schema loading and intermediate catalog construction. Native
compile/query failures propagate without Python binding fallback, automatic
retry or rollback. Explicit retry re-reads sources; a successful activation cache
entry still retains vector qualification. Stateful providers own their recovery.

## Intermediate visibility and donor scope

The additive `validate_intermediate` query contains `field` and `target`. A target
requires `source`, `derived`, `readable` and `dependencies`. Source is either
`{"kind":"dataset","name":"SRC"}` or `{"kind":"self","fields":["K"]}`;
the other fields are ordered name lists. A field is visible when stored or
derived and, if `readable` is nonempty, explicitly listed there. An empty readable
list means unrestricted visibility. An absent backing dataset adds no direct
field finding because its declaration already failed. Selecting a field in a
readable list cannot invent it. Dotted suffixes remain literal fields.

`validate_intermediate_read` contains `read`, requiring `reader`, `target_name`,
`field`, `donor_dataset`, `visible`, and an optional/null `target` in the shape
above. The core skips a reader naming itself (shared cycle analysis owns that
failure) and an unavailable target. Reading another SELF target produces a
`self_phase` finding and suppresses later findings. Otherwise the selected field
must be visible, followed by each target dependency in authored order. Bare
dependencies must occur in the donor's visible fields; qualified dependencies
must name that donor dataset and a visible literal suffix. Duplicate dependencies
retain their individual indices. Even after a field finding, unavailable
dependencies are reported. A missing donor target's backing dataset has no stored
fields, but its explicitly derived fields remain visible.

Both queries return `intermediate_validation` with ordered `diagnostics`:
`unknown_field`, `self_phase`, or `unavailable_dependency` with a zero-based
`dependency` index. Python attaches REQ-0125 field provenance or REQ-1263 donor
provenance at the existing sites. Expression traversal, dependency collection,
derivation typing and phase selection remain normalized host inputs; the query
does not execute an intermediate or authorize callback/data access.

Each query admits at most 65,536 written name entries and 1,048,576 total UTF-8
context bytes. Counts include query identity/field names and all target/donor
lists, including duplicates and context belonging to a skipped read. Limits
produce separate `intermediate_entries` or `intermediate_context_bytes` outcomes.
The request byte cap applies before decoding. A per-query donor-name index avoids
repeated linear membership scans and is discarded after validation. The owned
catalog remains unchanged after malformed, limited or repeated queries.

The frontend requires `intermediate_validation` before activation/source access;
a wheel advertising only earlier queries returns
`native_intermediate_reference_validation` unsupported status at `$`. There is no
fallback to Python rules after selecting the native compiler.

## Key relations

The `key_relations` feature adds two metadata queries. `comparable_types` takes
`left` and `right` known static types and returns `kind: comparable_types` with a
`comparable` boolean. Exact types compare; `int` and `float` also compare in
either direction. `date` and `datetime` remain distinct. Known `bool` expression results compare only with `bool`; this does not add a
boolean column type to catalogs or inferred-key fields. Unknown static expression
types are deferred by the host before this query, without guessing a type.

`infer_keys` takes ordered output `keys` and a `fields` list of `{name, type}`
right-side declarations, including normalized SELF/derived fields when applicable.
It consults the immutable output catalog and returns `kind: key_inference` with
an `inference` tagged by `kind`: `keys` (original key indices),
`no_applicable_keys`, `undeclared_output` (key index), or `incompatible`
(first mismatched key index, `expected` and `actual` declared types).
Selection and mismatch order follow the output keys, independently of right-field
order. Names are literal and Unicode is not normalized. Duplicate key entries
remain distinct indices; duplicate right fields and empty names are invalid
metadata. All submitted names count toward 65,536 entries and 1,048,576 UTF-8
bytes before semantic early exits, including unused fields and repeated keys.

Native planning uses these queries for implicit and omitted-key inference and
declared pair compatibility in row joins, aggregates, lookups and range bounds.
Hosts still determine expression types, row/group availability, unknown-name
priority, relation dependencies and diagnostic provenance. An invalid root key
now receives REQ-0220 `undeclared_column` at `keys[i]` before source access in both
frontends. Direct planning also refuses inference for invalid output identity,
preventing the previous `KeyError` when a source carried an undeclared output key.
Unresolved inherited declarations wait for parent normalization.

The frontend requires `key_relations` before activation/source access; previous
query sets return `native_key_relations` unsupported at `$`. A query error aborts
planning without executing a partial plan or falling back to Python. Explicit
retry reacquires sources; earlier provider effects are not rolled back.

## Evidence and remaining boundaries

Twenty-nine independent shared cases pin exact bindings, literal dotted names,
Unicode, suggestions, phase/type priority and declaration identities. Rust core,
transport, installed Python batch/prepared interfaces and installed R replay
authored truth. Prepared calls remain valid after their original request is
discarded. Admission tests cover metadata errors, limits and subsequent reuse.
Another 29 authored scope cases replay through Rust, both installed native
interfaces, R and the unchanged default Python rules. They cover driver/existence
priority, join exemptions/suppression, grouped-row and multi-template column grain,
empty groups, duplicate drivers and combined scope/type findings. Installed
planning tests pin full diagnostic requirements, contexts and paths with the
reference qualified-field helper disabled, including unchanged original CSVs.
Thirty-one additional authored intermediate cases cover stored/derived/SELF
visibility, declared-column restrictions, donor dependencies, duplicate indices,
Unicode, absent targets, phase priority and self-cycle deferral. The same corpus
runs through both installed hosts and the unchanged default Python rules.
Installed planning tests disable both reference intermediate helpers and compare
full diagnostic paths, contexts, requirements and combined finding order.
Fifty authored key cases cover all 36 known-type pairs (including expression
booleans) plus ordered
inference, numeric compatibility, first mismatches, invalid identity, empty
intersections, literal Unicode/dotted names and duplicate key indices. The same
truth is replayed by the default Python rules and native batch/prepared services.

Installed Python planning checks exact diagnostic paths, requirements and
contexts against explicit expectations and the unchanged default planner.
The original ADLB, window, lookup and project-function CSVs run with reference
binding and bare-name fallback disabled. Tests also cover pre-effect admission,
captured service identity, compile/query failures and explicit project retry.
CI runs installed tests against both the wheel and an independently rebuilt
source archive, plus the installed R source package.

This service does not own YAML/schema loading, normalization, derivation
inheritance, phase selection, join construction, intermediate expression
traversal/dependency collection, expression typing or typed dataset lowering. Bare-name validation is
connected to row and column output-expression checks; other scope-specific
validation remains in the host. R current-schema compilation, full language and
workflow coverage, numerical policy, performance and release gates remain open.
