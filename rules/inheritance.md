# Inheritance

A specification may inherit from parent layers. Composition turns the entry
file and its layers into one resolved specification before validation and
execution.

## Layers

<a id="layer-1"></a>

### LAYER-1. A specification is its entry file composed with every layer it reaches

The file requested for validation or execution is the entry file. The entry
file and every file reached through `parents` are layers; each contributes the
root fields and keyed declarations it writes, and composition produces one
resolved specification. An entry file without `parents` is a composition of
one layer and is resolved, pruned and validated on the same terms.

Replaces: REQ-0614, REQ-0615, REQ-0616.

A field is present when its mapping key is written, including when its value
is null. Absence inherits; presence replaces or clears under
[LAYER-5](#layer-5) to [LAYER-9](#layer-9). The resolver keeps the provenance
of every contributed value, down to the leaf a layer wrote, while it works.
Provenance is diagnostic state, not a field of the resolved specification; for
a relative path it also records the declaring layer and that layer's spelling,
which [Resource resolution](storage/resources.md#req-0780) resolves once.

<a id="layer-2"></a>

### LAYER-2. Parents are local YAML files resolved from the layer that names them

`parents` is an ordered list of paths; a single path is shorthand for a list of
one. A relative path resolves from the directory of the layer that declares it,
and an absolute local path is allowed. A URL or URI, including `file://`, is
not a parent path. Two spellings that reach the same file, after symbolic links
are resolved, are one layer.

Fails: `invalid_parent_path`, `parent_not_found`.
Replaces: REQ-0617, REQ-0618, REQ-0619, REQ-0653, REQ-0654.

A URL, URI, empty path or other non-local reference fails with
`invalid_parent_path`, reporting the declaring file and its `parents` entry. A
missing, unreadable or non-regular parent fails with `parent_not_found`,
reporting the declaring file and path.

<a id="layer-3"></a>

### LAYER-3. Layers contribute depth first, once each, later layers taking precedence

Starting at the entry file, each layer's parents are visited left to right,
depth first, and then the layer itself. A layer contributes once, at its first
visit, and later contributions take precedence over earlier ones. Reaching a
layer that is already on the active traversal path is a cycle.

Fails: `inheritance_cycle`.
Replaces: REQ-0620, REQ-0621, REQ-0655.

For parents `A` then `B` that both inherit `Common`, the contribution order is
`Common -> A -> B -> entry`. A difference between two parents is resolved by
this order, not by a conflict error. Reaching a layer whose contribution is
already complete skips it. `parents` is consumed by the traversal and is absent
from the resolved specification. A cycle reports the complete canonical path
cycle.

<a id="layer-4"></a>

### LAYER-4. Every layer is a valid fragment of the active schema version

Each layer must be a non-empty mapping that declares `schema_version` equal to
the active bundle version. A layer need not be complete: requiredness is
checked on the resolved specification, but unknown fields and invalid values
fail in the layer that writes them. Two members of one keyed collection in one
layer must not share an identifier.

Fails: `schema_version_mismatch`, `duplicate_identifier`.
Replaces: REQ-0622, REQ-0623, REQ-0624, REQ-0626, REQ-0656, REQ-0658, REQ-0659.

Requiredness is deferred for every root field, for the direct members of
`input`, `intermediates`, `columns` and `rows`, and for every depth inside a
`columns` member. An entry file may therefore omit `output` and inherit it. The
mapping key identifies an `input` member; a member of `intermediates`,
`columns` or `rows` carries its `id`, `name` or `id`. A version mismatch fails
before composition and reports every implicated file and value; inheritance
never migrates versions. A malformed fragment fails under
[Schema language](reference/schema-language.md) at its contributing file and
field. [Schema language](reference/schema-language.md) shorthand is expanded in
every supplied non-null field before composition, so equivalent spellings
contribute the same value.

## Composition

<a id="layer-5"></a>

### LAYER-5. Root fields replace whole; keyed collections compose by identifier

A later layer's non-null root field replaces the accumulated value whole,
including every mapping and list nested in it. `input`, `intermediates`,
`columns`, `rows` and `windows` instead compose member by member: a new
identifier appends in contribution order, and a matching identifier keeps its
first position and merges under [LAYER-6](#layer-6) to [LAYER-8](#layer-8).

Replaces: REQ-0627, REQ-0628, REQ-0629, REQ-0631, REQ-0642.

| Root field | Composes |
|---|---|
| `schema_version` | must be identical in every layer |
| `parents` | traversal instruction; never contributed |
| `input` | by dataset identifier |
| `intermediates` | by `id` |
| `columns` | by `name` |
| `rows` | by `id` |
| `windows` | by name; a supplied definition replaces the whole definition |
| every other root field | complete replacement |

Later root `metadata`, `keys`, `output` and `verifications` therefore replace
their inherited values whole. An entry that inherits `output` publishes at the
location its inherited `relative_to` selects, and its `path`, `warning_log` and
`verification_log` keep the declaring layer's provenance under
[LAYER-10](#layer-10); an entry can inherit `relative_to: entry` or replace
`output` whole. Dataset shorthand is expanded before datasets are matched: a
bare path becomes the long dataset form, so a later `relative_to` governs an
inherited `path` and `schema` without restating them.

<a id="layer-6"></a>

### LAYER-6. A matching input, intermediate or row merges its immediate fields

When a later layer writes a member of `input`, `intermediates` or `rows` whose
identifier is already present, an absent member field is inherited and a
present non-null member field replaces the inherited field whole. Nested
classes, mappings, lists and registry values inside that field are not
patched.

Replaces: REQ-0625.

A non-keyed root field supplied by a layer likewise validates as one complete
field value.

<a id="layer-7"></a>

### LAYER-7. A matching column merges field by field according to each field's kind

When a later layer writes a `columns` member whose name is already present,
each present non-null field composes with the value it inherits: a class field
by field, a mapping key by key, a registry value only when both name the same
keyword, and every scalar and list by replacement. A value of a different kind
replaces what it inherits.

Replaces: REQ-0630.

| Kind | Composes by |
|---|---|
| Class | field by field, recursively |
| Mapping, `dict[K, V]` | key by key; each value composes by its own kind |
| Registry value | its one keyword; different keywords replace whole |
| Every scalar and every list | replacement |

A field the accumulated member does not carry is taken whole. Every list
replaces: a column's `verifications`, a lookup or aggregate `key` in any form,
and ordered arguments such as `str_concat.sources`, `order_by`,
`first_available.sources` and the `cut` breaks and labels. Every leaf a
`columns` patch writes validates normally; only requiredness is deferred, so a
required leaf that no layer supplies fails on the resolved specification. A
schema default is applied to the composed value, not to each contribution, so a
later layer that never mentions a field cannot replace an earlier value with
the default. A child may thus change only `AVAL.label`, add one key to an
inherited `AVAL.metadata`, or add `unconvertible` to an inherited
`AVAL.derivation` without restating its expression; a derivation naming a
different keyword replaces the whole derivation.

<a id="layer-8"></a>

### LAYER-8. A named window composes by name and is replaced whole

Root `windows` composes by name and keeps other inherited names. A later
definition of the same name replaces that whole window, whose omitted fields
take ordinary window meanings. Every reference, inherited or not, uses the
final composed definition.

Replaces: REQ-1254.

A window name is not a variable or dataset dependency. Root `windows: null`
clears the collection under [LAYER-9](#layer-9); an individual definition
cannot be null. A named reference and an inline window are different kinds,
so replacing one with the other replaces the complete `window` value.

<a id="layer-9"></a>

### LAYER-9. A null at a composition boundary clears an inherited optional field

YAML null written for an immediate root field or keyed-member field clears the
inherited value; the field becomes absent. When nothing is inherited, a first
null is an ordinary declared value and keeps its schema meaning. Clearing a
required field, an identity field or `schema_version` fails. There is no
separate remove or drop construct.

Fails: `invalid_clear`.
Replaces: REQ-0632, REQ-0633, REQ-0634, REQ-0660.

The boundary does not move inside a composing `columns` member. A first
declaration such as an intermediate's `no_match: null` answers with missing,
including in a layer with `parents: []` or a parent inherited by an empty
child; a later layer's `no_match: null` over an inherited `no_match` clears the
handler, even if the inherited value is null. Below the boundary a null keeps
its schema meaning at every depth: `derivation: {literal: null}` replaces the
derivation with a literal missing value, and `source.absent`, `mapping.missing`
and `unconvertible` declare the missing value a handler substitutes. A layer
therefore clears a whole member field and restates what it keeps; it cannot
remove one key of an inherited mapping or one field of an inherited class. A
null on a field whose schema does not accept null still fails that field's
value constraint. `invalid_clear` reports the field and contributing file.

<a id="layer-10"></a>

### LAYER-10. A path keeps its declaring layer's base and authored spelling

A contributed value whose schema type is `path` is interpreted at the base its
owning rule selects: the declaring layer by default, the entry file when
`relative_to: entry` selects it. Composition keeps the authored spelling and
provenance. A materialized resolved specification writes an inherited relative
path relative to the entry file, preserving the location it names.

Replaces: REQ-0635, REQ-0636, REQ-0637.

If the platform cannot express that location relative to the entry file, the
canonical absolute local path is written. An absolute contributed path is
materialized exactly as written. `parents` paths are not materialized.
Rebasing states where a file is, not whether a run may read it:
[Resource resolution](storage/resources.md) accepts or rejects the rebased
path at the location it names, and a missing file fails there without retrying
another spelling.

## The resolved specification

<a id="layer-11"></a>

### LAYER-11. Declarations that cannot affect the artifact or an assertion are pruned

After composition, the resolver keeps only declarations reachable from the
semantic roots and removes the rest, whether or not the entry file declares
`parents`. The roots are the columns named by `output.columns`, `keys` or
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
contribution position comes first, which keeps `Common`, earlier parent and
child order for independent columns. `output.columns` is never reordered.

Replaces: REQ-0643, REQ-0644.

A derivation that reads an undeclared column, or that reads itself directly or
through other columns, fails under the
[Column derivation](columns.md); ordering repairs neither.
`output.columns` alone defines artifact membership and column order under
[Output](output.md#output-1).

<a id="layer-14"></a>

### LAYER-14. The resolved specification is canonical

The resolved specification contains no `parents` and no clearing nulls, uses
the long form of every shorthand, holds only reachable keyed declarations,
lists columns in the order of [LAYER-13](#layer-13), and keeps root and member
fields in schema order. Conformance compares its YAML data tree, not its
layout.

Replaces: REQ-0645, REQ-0646, REQ-0647, REQ-0648, REQ-0649, REQ-0650,
REQ-0652.

A free-form mapping whose owning field was replaced whole keeps the order of
the contribution that supplied it.
