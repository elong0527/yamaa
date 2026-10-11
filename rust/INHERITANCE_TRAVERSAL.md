# Shared inheritance traversal

The engine owns demand-driven inheritance traversal through native source ports.
`inheritance_capture::prepare` captures/decode parents once and preserves their
original snapshots; `inheritance_preparation::prepare` admits layers, composes,
expands windows, prunes and normalizes in deterministic order. Public Python/R
build and check routes use this application through native file preparation.

Python's module `traverse_inheritance` and frozen `_Schema.traverse_inheritance`
JSON callback probes are retired. Existing `_prepare_document` and
`_prepare_inherited_specification` bindings accept separate typed canonicalize,
raw-byte capture and lexical rebase callbacks. The optional compatibility
`NativeSchemaInterpreter` refuses its old hook with `inheritance_traversal`
before encoding or source effects; it does not fall back to reference traversal.
The default reference and custom typed interpreter/resolver behavior is unchanged.

The remaining R component probe and shared `inheritance/1` bridge below retain
their existing protocol for separate owner-coordinated retirement. Python-only
removal does not close #1755's No JSON host callback criterion. Retained schema/1
queries carry data but no host source callback authority.

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
host failures preserve their original R condition, including
interrupts, after the Rust call returns. The historical Python bridge mapped only
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
admission failures retain `source`, `entry` and `context_document` so diagnostic value
references address that child's authored input. Cycles carry the ordered
canonical `cycle` context. Version findings retain entry/source identities and
role-specific expected/actual version context. Unavailable-source diagnostics
retain the written parent before resolution or the display path after resolution.
The historical Python exception bridge copied the outcome's `source`, `entry` and `parent`
identities into each diagnostic context; it retains all existing value and
version context, including a null actual version for a missing declaration.
The reference loader reports the same identities. Custom admitted schemas whose
control fields cannot drive traversal return
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

R still exposes the component `yamaa::traverse_inheritance(schema_request,
request, callback)`. Stateless calls compile the schema before invoking source
callbacks; invalid schema outcomes carry `status: "invalid_schema"` and the
original `schema_outcome`. Its registration/export and the shared JSON callback
transport remain pending removal. No Python registration or snapshot method
remains for that protocol.

Native application callbacks preserve synchronous source operations, remaining
byte/text allowances and original host exceptions/interrupts. They do not parse
JSON host operations. Physical YAML decoding, filesystem identity and lexical
path rebasing stay at the native resource/codec boundary; engine services own
capture, traversal and preparation ordering. Current application byte budgets
remain distinct from the removed Python JSON request/reply budgets.

## Qualification and remaining work

Eight complete outcomes and source traces in `inheritance_traversal.tsv` and
`inheritance_sources.tsv` remain byte-identical independent oracles for Rust and
the remaining installed R probe. Seven complete raw preparation rejection/effect
oracles are also preserved through both hosts' original-document suites.

The installed Python inheritance suite now checks both existing typed preparation
bindings: complete original reports/CSV, source capture order/allowances, exact
exception and control-flow identity, same-thread reentrancy, and recovery after
native reply shape/capture refusal. It asserts both retired attributes are absent
and retained schema query behavior agrees. The installed schema-loading suite
keeps supported inherited/window CSV truth and complete failure contexts. Full
normalized/pruned documents, raw/default origins, committed resolved artifacts
and window definition/override provenance use held typed adapter arenas.

COUNT scope, mapping and explicit-null LOOK fixtures remain admitted by pure
native document preparation but encounter current compiler Unsupported gates
before study authority. Their complete preparation/provenance expectations are
preserved; this retirement does not claim execution parity for those gates.
Prior optional-loader and JSON callback results are historical component evidence.
Private analyze/interpret probes and reference/custom schema dispositions remain
separate API work. No authored source, expected report or CSV is regenerated.

This service does not qualify full R workflows, final shared model validation,
remaining dataset execution scopes or issue #1585's deployment/release gates.
POWER/EXP/LN numerical compatibility requires separate qualification. No reference
golden or comparison tolerance is changed to accommodate Rust behavior.
