# Shared decoded schema service

The experimental `schema/1` service admits a complete decoded schema bundle,
validates its defaults and interprets decoded documents through `yamaa-core`.
It is available as Python `yamaa_native.interpret_schema` and R
`yamaanative::interpret_schema`. Neither entry point reads YAML, opens files,
activates artifacts, invokes callbacks or reads source tables.

Python also exposes `_compile_schema(request)`, returning `(snapshot, outcome)`.
Only successful admission produces a frozen `_Schema` snapshot. Its
`analyze(request)` method owns all schema data and starts a fresh request budget;
mutating or releasing the original request cannot change it. R's stateless
batch path uses the same compiler and evaluator.
Successful compilation retains default admission on the immutable core schema.
Normalization queries still validate their written input and charge actual
normalization work, but do not revalidate unused defaults against the query
budget. A core structure that has not completed default preparation retains
the validating normalization path; failed or interrupted preparation cannot
establish the prepared state.

The default specification loader still uses the Python interpreter. The explicit
`yamaa.adapters.native_specification.load_specification` path captures the Rust
service before YAML IO and uses it throughout schema admission, validation,
normalization, inheritance fragments and window type selection. The host keeps
YAML decoding, filesystem authority, composition, dependency discovery, window
expansion and model validation. Current R specification workflows still need
integration. Python remains the default and `execution_supported` remains false.

## Requests

Every envelope requires `"protocol":"schema/1"`. Unknown or duplicate object
fields, unknown operations, invalid UTF-8 and malformed decoded trees are
transport failures, not language findings. Callers cannot set resource policies.

Compilation requires `schema`, containing:

- `modules`: an ordered array of `{name, document}` objects;
- `entry`: the module index used as the entry point;
- `root_class`: the requested root class name.

The complete transitive include closure must be provided. Rust checks declared
include names, closure, cycles, versions, declarations, references, field reuse
and descriptors. The host remains responsible for filesystem confinement,
symlink authority, duplicate YAML keys, alias rejection and YAML 1.2 decoding.

The stateless entry point additionally requires `queries`. A prepared snapshot's
request contains only `protocol` and `queries`. The closed operations are:

| Operation | Additional fields |
| --- | --- |
| `validate_document` | `document` |
| `normalize_document` | `document` |
| `validate_descriptor` | `descriptor`, `document`, `fragment`, `path` |
| `normalize_descriptor` | `descriptor`, `document`, `fragment` |
| `matching_member` | `descriptor`, `document`, `fragment` |
| `validate_types` | `types`, `document`, `fragment`, `path` |
| `normalize_types` | `types`, `document`, `fragment` |
| `matching_types` | `types`, `document`, `fragment` |

Descriptor identifiers come from the compilation response. A descriptor query
interprets the root of its supplied decoded document. `matching_member` selects
the first matching written type without applying shorthand. Fragments suppress
requiredness and default materialization at every depth; other checks remain.
The projected `types` operations bind an ordered array of type expressions to
the captured schema. Rust parses and resolves every name before interpreting
values. An empty selection is useful after inheritance/window cycle filtering
and `matching_types` returns a null member. Parsing and attempted branches share
the batch's validation budget; unknown names and malformed syntax are query
errors, while parser resource refusals retain their policy category.

## Decoded documents

A document is `{nodes, root}`. Children precede parents and every non-root node
has exactly one owner. Sharing, cycles, unreachable nodes, duplicate scalar
keys and non-scalar keys are rejected before interpretation.

| Node | Fields |
| --- | --- |
| `null` | `kind` only |
| `boolean` | `kind`, boolean `value` |
| `integer` | `kind`, canonical decimal string `value` |
| `float` | `kind`, 16 lowercase hexadecimal binary64 `bits` |
| `text` | `kind`, UTF-8 string `value` |
| `sequence` | `kind`, child-index array `items` |
| `mapping` | `kind`, ordered `[key_index, value_index]` array `entries` |

Integers retain arbitrary width. Booleans do not become integers during type
validation; integer values can satisfy `float`. Mapping-key equality follows
exact decoded scalar equality, including bool/integer/integral-float collisions,
without rounding large integers or hashing content.

Float bits must describe a finite value. Decimal JSON numbers cannot substitute
for the bit string. This preserves negative zero and every finite binary64 value
without depending on a host JSON parser's decimal conversion.

## Outcomes and ownership

Responses contain `protocol` and `outcome`. Compilation returns `compiled`
metadata, `invalid_schema` issues, `invalid_defaults`, `resource_limit` or
`unsupported`. Metadata includes class fields, aliases, registries, descriptor
constraints, original descriptor `source_node` occurrences (including reused
fields), default-node origins and `diagnostic_unicode_version`. Failed
default admission includes metadata so its descriptor references are resolvable;
it never returns a prepared handle.

A query batch returns `analyzed` with ordered `results`. Stateless responses
also include the captured `schema` metadata. Each result is `valid`, `invalid`,
`normalized`, `matched`, `resource_limit` or `unsupported`. A normalized result
contains a decoded `document` and a parallel `origins` array. Each origin names
an input occurrence or a module/default descriptor occurrence and records
whether shorthand generated the node. Recursive default materialization creates
fresh occurrence identities and reaches an explicit resource refusal rather
than silently truncating defaults.

Diagnostics preserve ordered paths, conditions, requirements and context
entries. Small context values are tagged `text`, `count` or `null`. Larger
values stay tagged references: `input_value` names a node in that query's input;
descriptor constraint references name captured metadata. For invalid defaults,
input-node references belong to the declaring descriptor's schema module.
Hosts must resolve these references without lossy numeric conversion.

## Policies and qualification

Requests are capped at 8,388,608 UTF-8 bytes before JSON decoding. Responses
are capped at 16,777,216 bytes, with incremental batch accounting and a separate
final serialization check. A batch has at most 256 queries. Core bundle,
descriptor, regex, validation and normalization policies remain separate.
Validation work, attempted diagnostic allocations and normalization storage
accumulate across queries; unsuccessful union attempts do not refund them.
These are logical policies, not a process-memory or cancellation guarantee.

Six complete wire fixtures are authored independently and replayed through
Rust and the installed Python/R packages. Core tests cover recursive aliases,
version and diagnostic priority, constraints, fragments, union selection,
default provenance and both specified shorthand shapes. Supplemental local
observations over all 310 committed benchmark specifications and 14 schema
modules matched the Python reference in both installed hosts: 296 normalized
documents and 14 ordered diagnostic results. This does not qualify execution,
inheritance resolution, every possible schema or the full release gates.

`installed_schema_loading.py` additionally disables the reference schema
interpreter while loading installed Python packages: three inheritance and
composition cases compare to committed resolved documents, and three loaded
specifications execute through Rust to unchanged expected CSV bytes. These
bounded workflow checks do not qualify all schemas or replace the full release
gates. The optional Python bridge rejects uncaptured constraint-bearing
descriptors and mismatched compiled root classes explicitly, without fallback.
It accepts type-only projections used by inheritance and windows. The host
closure reader caps source bytes at 8 MiB, confines include basenames, rejects
symlink entry points/includes and preserves the existing YAML restrictions.
Structured admission findings remain attached to schema errors; exhaustive
malformed-schema prose compatibility still requires qualification.

Two explicit compatibility items remain open:

- Compound malformed-name labels use Python-style quoting with pinned Unicode
  18.0.0 printability. The lookup is checked for every scalar against the pinned
  authoritative data. Python's Unicode 16.0.0 table differs for 17,810 scalars,
  affecting representation inside these diagnostic labels. This is not an
  admission or value-normalization difference and does not change the default
  Python backend. Display compatibility requires release qualification.
- A schema shorthand that would normalize a mapping key into a list or mapping
  returns `unsupported` with feature `normalized_mapping_key`. It never drops
  the entry or claims successful normalization. The reference's corresponding
  host `TypeError` also needs compatibility qualification.

The fixture comparisons and corpus observations do not replace exact-final-head
review, hosted CI, full host workflow integration or the independent output and
release gates in issue #1585.
