# Shared inheritance traversal

The experimental `inheritance/1` service owns demand-driven inheritance traversal
in `yamaa-engine`. It admits each decoded layer against the captured schema,
checks entry/parent versions and returns normalized contributions in deterministic
postorder. Python's opt-in native loader uses it. Python remains the default
backend and `execution_supported` remains false.

This is a separate application service from `schema/1`. Schema queries retain
no IO authority. Traversal receives an explicit synchronous source callback;
Rust never opens files or discovers an ambient reader. The host owns filesystem
confinement, canonical identities, source snapshots and decoding. R exposes the
same service but still needs a complete current-schema workflow loader.

## Source protocol and ordering

The initial request has three fields:

```json
{"protocol":"inheritance/1","entry":{"identity":"/entry.yaml","display_path":"/entry.yaml"},"document":{"nodes":[{"kind":"mapping","entries":[]}],"root":0}}
```

The displayed empty mapping illustrates the envelope; an admitted layer also
needs its schema-required fields. `document` uses the owned decoded tree format
specified in [schema transport](SCHEMA_TRANSPORT.md).
The entry document is already captured and is never read again. Canonical
identities are compared directly, without content hashing. The host must supply
stable canonical identity strings, including its filesystem's symlink and case
rules; `display_path` retains the path spelling used for read-error context.

For each written parent occurrence, the callback receives a canonicalization
request, followed by a read request only if the source needs visiting:

```json
{"protocol":"inheritance/1","operation":"canonicalize","declaring":"/entry.yaml","path":"base.yaml"}
{"protocol":"inheritance/1","operation":"read","identity":"/base.yaml","display_path":"/base.yaml"}
```

Canonicalization resolves relative paths against the declaring canonical file's
directory and verifies a regular local file. Its successful reply contains
`outcome: {"status":"resolved","identity":"/base.yaml","display_path":"/base.yaml"}`.
A successful read reply contains `outcome: {"status":"document","document": TREE}`.
Both include `"protocol":"inheritance/1"`. Either operation can return
`outcome: {"status":"unavailable"}` for an unavailable source. Unknown/duplicate
fields, wrong-operation replies, invalid trees and invalid UTF-8 are refused.

The callback also receives the remaining cumulative reply-byte allowance. It
runs synchronously on the calling host thread. Decode errors and other raised
host failures preserve their original Python exception or R condition, including
interrupts, after the Rust call returns. The Python filesystem adapter maps only
`OSError` to unavailable. No retry or later sibling read follows a failure.

The traversal order is:

1. Validate and normalize the complete layer before checking its version or
   requesting any parent. Structural failures retain their diagnostic priority.
2. Visit normalized parents left-to-right. Refuse URI references before source
   callbacks; preserve local drive-rooted path syntax.
3. Canonicalize every written occurrence. Check the active path for a cycle
   before checking completed sources. A cycle includes both repeated endpoints
   and is reported before rereading that source.
4. Skip a completed canonical source. Otherwise read, admit and visit it.
5. Emit each source once, after its parents. A diamond's shared ancestor retains
   its first contribution position.

The successful outcome is `{"status":"traversed","layers":[...]}`. Each layer
contains `identity`, `display_path` and its owned normalized `document`. No
partial successful graph escapes a failure. Source effects already performed
are not rolled back. Reentrant calls use independent graph state and budgets.

## Diagnostics, limits and adapters

Language failures use the existing ordered schema diagnostic envelope. Child
admission failures retain `source` and `context_document` so diagnostic value
references address that child's authored input. Cycles carry the ordered
canonical `cycle` context. Version findings retain entry/source identities and
role-specific expected/actual version context. Unavailable-source diagnostics
retain the written parent before resolution or the display path after resolution.
Custom admitted schemas whose control fields cannot drive traversal return
`unsupported`; there is no fallback to another graph implementation.

Each request has an 8 MiB initial request cap, 8 MiB cumulative callback-reply cap
and 16 MiB response cap. Logical graph budgets additionally bound 256 layers,
8,192 parent occurrences, depth 64, 1 MiB path/identity text, 131,072 input nodes,
8 MiB input text and 8 MiB work units. All layers share the normalization budget.
The Rust API allows callers to reuse budgets across attempts; failed attempts
retain charges and checked counters cannot wrap into success. Host entry points
start fresh budgets. These limits do not establish process-memory containment or
asynchronous cancellation guarantees. Host bindings refuse oversized replies
before copying them into Rust-owned response text.

Python exposes `yamaa_native.traverse_inheritance(schema_request, request, callback)`
and frozen `_Schema.traverse_inheritance(request, callback)`. R exposes
`yamaanative::traverse_inheritance(schema_request, request, callback)`. Stateless
calls compile the schema before invoking source callbacks. Invalid schema outcomes
carry `status: "invalid_schema"` and the original `schema_outcome`.

The Python loader supplies filesystem and captured YAML-decoder ports. After
traversal, it attaches written path provenance and rebases paths lexically using
the already canonical source directories, without reopening filesystem identity.
Data-file symlink targets are not resolved by rebasing. Shared composition,
window expansion and dependency resolution follow as before. Final host model
validation remains an explicit integration boundary.

## Qualification and remaining work

Seven complete outcomes and source traces in `inheritance_traversal.tsv` and
`inheritance_sources.tsv` are independently authored and replayed unchanged by
Rust and both installed hosts. Focused engine/transport tests also specify aliases,
cycles, complete layer admission, version/failure precedence and resource limits.
Installed Python loader checks disable reference layer/parent helpers and retain
the committed resolved documents and CSV expectations. Independent installed
Python/R tests exercise source traces, owned results, original host failures,
reply limits and reentrancy. Native CI runs the installed boundary checks for
direct/source-rebuilt Python artifacts and the staged R source package.

This service does not qualify full R workflows, final shared model validation,
remaining dataset execution scopes or issue #1585's deployment/release gates.
POWER/EXP/LN numerical compatibility requires separate qualification. No reference
golden or comparison tolerance is changed to accommodate Rust behavior.
