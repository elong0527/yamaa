# Shared named-window expansion

The experimental `schema/1` operation `expand_windows` moves schema-directed
named-window expansion into the shared core. Both installed host services call
`SchemaStructure::expand_named_windows`; the optional Python loader delegates
through its captured schema interpreter. Python remains the default and
`execution_supported` remains false.

## Semantics and provenance

The input is a normalized, composed decoded document. Root definitions are
validated even if unused. The walker follows declared classes, aliases,
registries, lists, dictionaries and ordered union selection. Arbitrary metadata
is not searched for strings that look like window names.

Each reference becomes an independent copy of its final definition. The operation
does not bind a row set or change caller scope. Declaration traversal determines
diagnostic order; input mapping order is preserved in the resulting tree.
`strict: false` expands known references while keeping unknown names and root
definitions for inheritance dependency discovery and pruning. `strict: true`
rejects surviving unknown names with `unknown_window`, `REQ-1253` and a logical
use-site path, then removes root definitions. Unknown-name diagnostics omit the
normalization-only `derivation.value` wrapper.

An `expanded` result contains:

- `document`: an owned postorder arena with no shared child occurrences;
- `origins`: input node indices parallel to the output nodes. A replacement root
  retains the reference occurrence; its copied children point to definition nodes;
- `references`: ordered `{path, definition}` links. The host retains layer/file
  identities and uses these links to attribute expanded fields to their original
  definitions, preserving the reference's own origin.

No partial document is published on invalid input or resource refusal. The
Python adapter stages provenance updates after receiving and decoding a successful
result; it does not fall back to host expansion on errors. Parent traversal,
layer composition, dependency pruning/order and host model validation still need
shared implementations. In particular, this operation consumes the final composed
definitions; it does not itself implement REQ-1254 layer composition.

## Policies and qualification

Expansion uses the schema batch's existing validation and normalization budgets.
Work, speculative union checks, diagnostic/path text and copied storage accumulate
across related queries. Failed attempts retain successful charges. Output nodes,
edges and text are charged before copying; recursion is capped independently of
the decoded document's depth. The existing 8 MiB request, 16 MiB response and
256-query limits remain in force. These logical quotas do not establish process
memory, cancellation or concurrent-workload containment guarantees.

Six independently authored complete wire cases in `schema_windows.tsv` replay
through Rust and the installed Python/R services. They specify copies, origins,
metadata preservation, strict/non-strict behavior and ordered unknown-name
findings. A batch-amplification regression proves three individually admissible
expansions cannot restart the aggregate storage budget. Fresh requests remain
independent.

Installed Python loader tests disable host schema/YAML interpretation in their
existing covered paths and disable the host window walker throughout. They retain
committed resolved documents and exact execution CSVs, and add inherited
whole-definition replacement, use/definition provenance and dead-reference
pruning. The source services are still captured before filesystem reads.

Row-template `row_value` window execution remains explicitly unsupported by the
native dataset frontend (`window_scope`). A dedicated fixture loads through the
shared compiler and verifies caller scopes against independently stated CSV using
the reference evaluator; native admission refuses it before source reads. This
is a recorded execution gap, not Rust execution qualification. Full current-schema
R workflows, shared inheritance/planning, numerical policy and issue #1585's
release matrix remain open.
