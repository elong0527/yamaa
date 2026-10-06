# Shared inherited-layer composition

The experimental `schema/1` operation `compose_layers` moves composition of
normalized inheritance fragments into `yamaa-core`. Both installed native hosts
expose it. The explicit Python native specification loader uses the captured
service and attaches retained file paths to its logical provenance. Python stays
the default backend and `execution_supported` stays false.

## Boundary and ordering

The query accepts an ordered `layers` array of decoded documents. Callers must
first admit each layer against the captured schema, normalize its fragments,
linearize the parent graph and rebase source paths. This operation does not open
files, traverse parents, check confinement, invoke callbacks or read tables.
Malformed normalized collection shapes and duplicate/missing keyed identities
are contract defects; this query is not a replacement for fragment admission.

`normalize_layer` admits a single authored contribution and supplies the
normalized fragment needed by composition. It validates the entire contribution
before normalizing any field. Invalid results contain all ordered findings with
their authored logical paths and no partial document. Unknown fields retain
input order; known fields follow schema order; duplicate identities follow the
corresponding member's findings. Diagnostic value references address the
original input tree.

Missing immediate fields remain deferred, except for the layer version and
keyed identities. Supplied non-column values must be complete; nested column
fragments defer requiredness and defaults. Optional null clear markers survive,
while null required fields or identities fail with `invalid_clear`. Input path
shorthand becomes an owned mapping with origins pointing to the authored text.
Parents receive normal descriptor validation and normalization. Parent graph
traversal and entry-versus-parent version consistency remain caller duties.

Both installed hosts expose this admission query through `schema/1`; Python's
opt-in loader delegates once per layer through its captured service. R callers
can now admit contributions before composition without a Python interpreter.
This does not supply a complete R filesystem or workflow loader. All admission,
normalization and composition queries share the batch's logical resource budget;
failed attempts retain their charges.

Root fields replace whole, except for keyed `input`, `columns`, `rows`,
`intermediates` and named `windows`. The `parents` field is omitted. Existing
members retain their first position and newly introduced members append in
contribution order. Input, row and intermediate members replace immediate
fields. Named windows replace their whole definition.

Column fields compose by the captured schema's first matching fragment kind:
classes and dictionary values merge recursively; the same registry keyword
composes its payload. Different kinds or registry keywords replace whole.
Lists and `match_key` always replace whole. Nested null remains an ordinary
value. Only immediate root/member null clears an inherited optional field.
Required fields, identities and fields that were never inherited reject clearing
with ordered `invalid_clear` findings. This includes newly introduced members.

The workflow resolver also accepts standalone documents. As in the ordinary
loader, an absent `parents` field selects complete-document normalization rather
than inheritance composition. This preserves an authored `no_match: null` as a
missing-value handler. An explicit `parents` field, including an empty list,
selects composition and its immediate-field clearing rules. Host file provenance
is retained for standalone documents without calling the composition service.

Defaults are withheld while layers compose. After the final layer, supplied
non-null column fields are materialized in schema field order. Missing whole
column fields and other incomplete declarations remain for subsequent pruning
and complete-document validation. Materialization errors retain their precedence
over collected clear findings.

## Ownership, diagnostics and limits

A successful `composed` result contains an independently owned decoded document
and written `{path, layer}` provenance. Merged containers retain their original
layer; replaced values identify the later layer. Generated defaults have no
invented written origin. Logical paths use the existing dotted host convention;
they are not arbitrary-key JSON pointers. Full custom-schema path compatibility
remains a qualification item.

An `invalid` result may retain `context_document` so `input_value` diagnostic
references address the composed input rather than an unrelated original layer.
This is error-rendering context only. The Python bridge raises before publishing
a resolved specification and never falls back to host composition on failure.

All temporary/output copies share the request's normalization storage, work and
depth policies. Matching kinds consumes the shared validation budget. Failed
queries retain their charges, and a new request starts fresh budgets. Transport
request/response caps still apply. These are logical limits, not a claim of
process-memory containment or asynchronous cancellation safety.

## Evidence and remaining work

Six complete wire cases have independently authored schema, input, output and
provenance expectations. Rust and installed Python/R replay exactly the same
records. Core tests additionally cover recursive kinds, registry changes,
occurrence ownership, ordering, invalid clears, deferred defaults and shared
resource refusals. Transport tests exercise cumulative copy limits across a
batch and fresh-request recovery.

Installed Python loader tests disable host composition and materialization,
along with the reference schema/YAML/window interpreters. Existing resolved
documents and expected CSV bytes remain unchanged. Four independent regressions
cover absent-field clears on new column/input/row/intermediate members; they
also fix the reference path's prior bypass of member clear checks.

Supplemental comparison over 310 current benchmark specifications observed 295
loaded documents and 15 matching invalid results, with no discrepancies. That
comparison supplies additional evidence, not independent expected truth.

Parent-graph orchestration, shared dependency discovery/pruning, full current
schema R workflows, unsupported execution scopes and issue #1585 release gates
remain open. This slice neither establishes full engine readiness nor qualifies
the separate POWER/EXP/LN numerical policy.
