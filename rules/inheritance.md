# Inheritance

A specification may inherit from a chain of parent layers. Composition turns
the entry file and its layers into one resolved specification before
validation and execution.

## Layers

<a id="layer-1"></a>

### LAYER-1. A specification is its entry file composed with the chain of parents it reaches

The file requested for validation or execution is the entry file. The entry
file and every file reached by following `parent` from it are layers; each
contributes the root fields and keyed declarations it writes, and composition
produces one resolved specification. An entry file without a `parent` is a
composition of one layer and is resolved, pruned and validated on the same
terms.

Evidence: `schema-inheritance`, `schema-inherited-output`.
Replaces: REQ-0614, REQ-0615, REQ-0616.

A field is present when its mapping key is written, including when its value
is null. Absence inherits; a written value replaces or merges, and a null
clears, under [LAYER-5](#layer-5) to [LAYER-9](#layer-9). The resolver keeps
the provenance of every contributed value, down to the leaf a layer wrote,
while it works. Provenance is diagnostic state, not a field of the resolved
specification; for a relative path it also records the declaring layer and
that layer's spelling, which [Resource resolution](reference/files.md#file-2)
resolves once.

<a id="layer-2"></a>

### LAYER-2. A parent is one local YAML file resolved from the layer that names it

`parent` is an optional single path naming the layer this layer inherits from.
A relative path resolves from the directory of the layer that declares it, and
an absolute local path is allowed. A URL or URI, including `file://`, is not a
parent path. Two spellings that reach the same file, after symbolic links are
resolved, are one layer.

Fails: `invalid_parent_path`, `parent_not_found`.
Evidence: `negative-remote-parent`, `schema-inheritance-project-root`.
Replaces: REQ-0617, REQ-0618, REQ-0619, REQ-0653, REQ-0654.

A URL, URI, empty path or other non-local reference fails with
`invalid_parent_path`, reporting the declaring file and its `parent`. A
missing, unreadable or non-regular parent fails with `parent_not_found`,
reporting the declaring file and path. A `parent` written as null is absent:
the layer has no parent, and an entry file whose `parent` is null is a
composition of one layer.

<a id="layer-3"></a>

### LAYER-3. Layers form one chain contributed from its root ancestor to the entry, later layers taking precedence

Starting at the entry file, each layer's `parent` names the next layer up the
chain, which ends at a layer without a parent. Layers contribute from that
root ancestor down to the entry file, and a later contribution takes
precedence over an earlier one. Reaching a layer that is already on the chain
is a cycle.

Fails: `inheritance_cycle`.
Evidence: `schema-inheritance`, `negative-cyclic-parent`.
Replaces: REQ-0620, REQ-0621, REQ-0655.

For a study entry whose parent is a compound layer whose parent is an
organization layer, the contribution order is
`organization -> compound -> study`. A difference between two layers is
resolved by this order, not by a conflict error. A layer is admitted under
[LAYER-4](#layer-4) before its parent is read, and at most 8192 parent links
are followed. `parent` is consumed by the traversal and is absent from the
resolved specification. A cycle reports the complete canonical path of the
cycle, from the layer reached twice back to itself, and whether the chain
returned to the entry file or to another layer on it.

<a id="layer-4"></a>

### LAYER-4. Every layer is a valid fragment of the active schema version

Each layer must be a non-empty mapping that declares `schema_version` equal to
the active bundle version. A layer need not be complete: requiredness is
checked on the resolved specification, but unknown fields and invalid values
fail in the layer that writes them. Two members of one keyed collection in one
layer must not share an identifier.

Fails: `schema_version_mismatch`, `duplicate_identifier`.
Evidence: `negative-version-mismatch`.
Replaces: REQ-0622, REQ-0623, REQ-0624, REQ-0626, REQ-0656, REQ-0658, REQ-0659.

Requiredness is deferred for every root field and at every depth inside a
member of `input`, `intermediates`, `columns`, `rows` and `windows`. An entry
file may therefore omit `output` and inherit it. The mapping key identifies an
`input` or `windows` member; a member of `intermediates`, `columns` or `rows`
carries its `id`, `name` or `id`. A version mismatch fails before composition
and reports every implicated file and value; inheritance never migrates
versions. A malformed fragment fails under
[Schema language](reference/schema-language.md) at its contributing file and
field. [Schema language](reference/schema-language.md) shorthand is expanded
in every supplied non-null field before composition, so equivalent spellings
contribute the same value.

## Composition

<a id="layer-5"></a>

### LAYER-5. Root fields replace whole; keyed collections compose member by member

A later layer's root field replaces the accumulated value whole, including
every mapping and list nested in it. `input`, `intermediates`, `columns`,
`rows` and `windows` instead compose member by member: a new identifier
appends in contribution order, and a matching identifier keeps its first
position and merges with the member it inherits under [LAYER-6](#layer-6).

Evidence: `schema-inherited-output`, `schema-inherited-input`.
Replaces: REQ-0627, REQ-0628, REQ-0629, REQ-0631, REQ-0642.

| Root field | Composes |
|---|---|
| `schema_version` | must be identical in every layer |
| `parent` | traversal instruction; never contributed |
| `input` | by dataset identifier |
| `intermediates` | by `id` |
| `columns` | by `name` |
| `rows` | by `id` |
| `windows` | by name |
| every other root field | complete replacement |

Later root `metadata`, `keys`, `output` and `verifications` therefore replace
their inherited values whole. An entry that inherits `output` publishes at the
location its inherited `relative_to` selects, and its `path` and `report`
keep the declaring layer's provenance under
[LAYER-10](#layer-10); an entry can inherit `relative_to: entry` or replace
`output` whole. Dataset shorthand is expanded before datasets are matched: a
bare path becomes the long dataset form, so a later `relative_to` governs an
inherited `path` and `schema` without restating them.

<a id="layer-6"></a>

### LAYER-6. A matching member merges recursively: classes by field, mappings by key, scalars and lists by replacement

When a later layer writes a member whose identifier is already present, each
field it writes composes with the value it inherits: a class field by field
and a mapping key by key, each value by its own kind; a registry value only
when both name the same keyword, and otherwise whole; every scalar and every
list by replacement. A value of a different kind replaces what it inherits,
and a field the member does not inherit is taken whole.

Evidence: `schema-column-composition`, `schema-inherited-input`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::recursive_patches_keep_positions_merge_mappings_replace_lists_and_defer_defaults`.
Replaces: REQ-0625, REQ-0630.

| Kind | Composes by |
|---|---|
| Class | field by field, recursively |
| Mapping, `dict[K, V]` | key by key; each value composes by its own kind |
| Registry value | its one keyword; a different keyword replaces whole |
| Every scalar and every list | replacement |

The rule is the same for every keyed collection. Every list replaces: a
column's `verifications`, a lookup or aggregate `key` written as a list, a
`case`, a window's `group_by` and `order_by`, and ordered arguments such as
`first_available.sources` and the `cut` breaks and labels. A lookup `key`
written as a mapping merges key by key. A child may thus
change only `AVAL.label`, add one key to an inherited `AVAL.metadata`, add
`unconvertible` to an inherited `AVAL.derivation` without restating its
expression, add one field type to an inherited `input` dataset, or add one
derivation to an inherited row template; a derivation naming a different
keyword replaces the inherited expression whole.

<a id="layer-7"></a>

### LAYER-7. Requiredness and defaults apply to the composed member, never to one contribution

Every leaf a layer writes inside a member validates in that layer, but
requiredness and schema defaults apply to the composed member. A required
field that no layer supplies fails on the resolved specification, and a later
layer that never mentions a field cannot replace an earlier value with the
field's default.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::complete_composition_materializes_defaults_once_and_keeps_written_provenance`, `rust/crates/yamaa-core/tests/schema_references.rs::keyed_member_fragments_defer_nested_requiredness_but_root_values_do_not`.

A whole root field is a complete value in the layer that writes it, so its
requiredness and defaults apply there. A default fills a field the composed
member does not carry, including one a later layer cleared under
[LAYER-9](#layer-9).

<a id="layer-8"></a>

### LAYER-8. A named window composes by name like every keyed member, and every reference uses the final definition

Root `windows` composes by name and keeps other inherited names. A later
definition of an inherited name merges with it under [LAYER-6](#layer-6), so a
layer can change one setting, such as `order_by`, and keep the others. Every
reference, inherited or not, uses the final composed definition.

Evidence: `rust/crates/yamaa-adapters/tests/declaration_contract_vectors.rs::complete_window_model_merges_parent_definition_with_child_fields_and_schema_defaults`.
Replaces: REQ-1254.

A window name is not a variable or dataset dependency. A setting the composed
definition omits takes its ordinary window meaning. Root `windows: null` clears
the collection under [LAYER-9](#layer-9); an individual definition cannot be
null. A named reference and an inline window are different kinds, so replacing
one with the other replaces the complete `window` value.

<a id="layer-9"></a>

### LAYER-9. YAML null makes a field absent, removing any value it inherits

YAML null written for a field or a mapping entry makes it absent: where a value
is inherited, at any depth composition reaches, the null removes it, and
anywhere else, including a first declaration, the field is simply absent.
Clearing a required field, an identity field or `schema_version` fails.
`{literal: null}` is the literal missing value, not a clear. There is no
separate remove or drop construct.

Fails: `invalid_clear`.
Evidence: `schema-inheritance-clear`, `negative-property-clear`.
Replaces: REQ-0632, REQ-0633, REQ-0634, REQ-0660.

The rule holds in every specification, whether or not it declares a `parent`,
and at every depth: inside a member, a whole root field or a list item. A layer
can therefore remove one key of an inherited mapping or one field of an
inherited class and keep the rest; a field whose schema has a default takes
that default again under [LAYER-7](#layer-7). A keyed member itself is never
null, and a list item or a registry payload is a value: the expression
`literal: null` is the missing value, and a handler, flag value, mapping entry
or function argument substitutes the missing value as `{literal: null}`
([COLUMN-10](columns.md#column-10)). `invalid_clear` reports the field and
contributing file.

<a id="layer-10"></a>

### LAYER-10. A path keeps its declaring layer's base and authored spelling

A contributed value whose schema type is `path` is interpreted at the base its
owning rule selects: the declaring layer by default, the entry file when
`relative_to: entry` selects it. Composition keeps the authored spelling and
provenance. A materialized resolved specification writes an inherited relative
path relative to the entry file, preserving the location it names.

Evidence: `schema-inheritance-project-root`.
Replaces: REQ-0635, REQ-0636, REQ-0637.

If the platform cannot express that location relative to the entry file, the
canonical absolute local path is written. An absolute contributed path is
materialized exactly as written. A `parent` path is not materialized.
Rebasing states where a file is, not whether a run may read it:
[File resolution](reference/files.md) accepts or rejects the rebased
path at the location it names, and a missing file fails there without retrying
another spelling.

## The resolved specification

<a id="layer-11"></a>

### LAYER-11. Declarations that cannot affect the artifact or an assertion are pruned

After composition, the resolver keeps only declarations reachable from the
semantic roots and removes the rest, whether or not the entry file declares a
`parent`. The roots are the columns named by `output.columns`, `keys` or
`output.order_by`, the columns dataset verifications read, every column or
intermediate that carries its own verification, every row template, and the
dataset `base` names.

Replaces: REQ-0638, REQ-0639, REQ-0640.

Definitions do not make themselves live. From the roots the resolver follows
every semantic reference recursively: variables in derivations and row
derivations, filters, grouping and ordering, the closed expression languages,
string-template placeholders, verifications and lookup matching. A qualified
variable makes its dataset or lookup live; a live lookup makes its dataset and
matching inputs live; a live row makes its driver, filter inputs, grouping
inputs and the derivations live columns need live. A row template is itself a
root, and `base` is live when a surviving row falls back to it. Dead entries
are removed from `input`, `intermediates` and `columns`, together with row
derivations that target dead columns.

<a id="layer-12"></a>

### LAYER-12. Name and reference validation applies after pruning

Structural validation applies to every field a layer writes, but requiredness,
name, reference and cross-field validation apply to the pruned resolved
specification. An unresolved reference reachable from a root fails; one inside
a pruned declaration is discarded with it. A failure is reported under the rule
that owns it, naming the entry specification and the contributing file and
field of each implicated value.

Fails: `missing_required_field`.
Replaces: REQ-0641, REQ-0651, REQ-0657, REQ-0661.

A declaration left out of `output.columns` remains as an internal column when a
derivation or verification uses it. A resolved specification to which no layer
contributed `output` fails as `missing_required_field` at `output`; an entry
file that omits `output` is not itself an error.

<a id="layer-13"></a>

### LAYER-13. Resolved columns follow dependency order with a stable tie-break

After pruning, each column is ordered after every column its derivation
reads. When more than one column can come next, the one with the earliest
contribution position comes first, which keeps an ancestor's independent
columns before its descendants'. `output.columns` is never reordered.

Replaces: REQ-0643, REQ-0644.

A derivation that reads an undeclared column, or that reads itself directly or
through other columns, fails under the
[Column derivation](columns.md); ordering repairs neither.
`output.columns` alone defines artifact membership and column order under
[Output](output.md#output-1).

<a id="layer-14"></a>

### LAYER-14. The resolved specification is canonical

The resolved specification contains no `parent` and no clearing nulls, uses
the long form of every shorthand, holds only reachable keyed declarations,
lists columns in the order of [LAYER-13](#layer-13), and keeps root and member
fields in schema order. Conformance compares its YAML data tree, not its
layout.

Replaces: REQ-0645, REQ-0646, REQ-0647, REQ-0648, REQ-0649, REQ-0650,
REQ-0652.

A free-form mapping whose owning field was replaced whole keeps the order of
the contribution that supplied it.
