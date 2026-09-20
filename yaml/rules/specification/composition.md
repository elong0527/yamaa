---
id: specification/composition
title: Specification composition
status: normative
---

# Specification composition

## Purpose

Resolve inherited layers into one minimal, ordered specification.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [Expression evaluation](../operations/expressions.md).
- [Schema language](../reference/schema-language.md).
- [Specification structure](structure.md).
- [Resource resolution](../storage/resources.md).

## Requirements

### Terms

<a id="req-0614"></a>

**REQ-0614.** The file requested for validation or execution is the **entry
file**. The entry file and every file reached through `parents` are
**layers**. A layer contributes the root fields and keyed declarations it
writes. The **resolved specification** is the complete mapping produced from
all contributions.

<a id="req-0615"></a>

**REQ-0615.** A field is **present** when its mapping key is written, including
when its YAML value is null. Absence inherits; presence replaces or clears as
defined below.

<a id="req-0616"></a>

**REQ-0616.** The resolver retains source provenance for every contributed value
while it works, down to the leaf a layer wrote, because a composed column
carries values from more than one layer. Provenance is diagnostic state and is
not a field of the resolved specification.

### Parent references

<a id="req-0617"></a>

**REQ-0617.** `parents` accepts one `path` or an ordered `list[path]`. A single
path is the [Schema language](../reference/schema-language.md) shorthand for a one-item list and is normalized before
traversal.

<a id="req-0618"></a>

**REQ-0618.** A parent is a local YAML file. A relative path is resolved from the
directory of the layer that declares it. An absolute local path is permitted. A
URL or URI, including `file://`, is not a parent path. A path that is missing,
unreadable, or not a regular file is an error.

<a id="req-0619"></a>

**REQ-0619.** The resolver canonicalizes an existing local path, including
symbolic links, before comparing file identity. Two spellings that reach the
same file identify one layer, not two.

### Linearization

<a id="req-0620"></a>

**REQ-0620.** Starting at the entry file, visit each layer's normalized parents
from left to right, depth first, and then visit the layer. A layer contributes
once at its first visit. Reaching a layer already on the active traversal path
is a cycle and fails. Reaching a layer whose contribution is complete skips it.

<a id="req-0621"></a>

**REQ-0621.** For parents `A` then `B`, where both inherit `Common`, the
contribution order is:

```text
Common -> A -> B -> entry
```

Later contributions have higher precedence. A difference between two parents is
resolved by order, not by a parent-conflict error. `parents` is
consumed during traversal and is absent from the resolved specification.

### Layer validation

<a id="req-0622"></a>

**REQ-0622.** Every layer is parsed under [Schema language](../reference/schema-language.md) and must be a non-empty mapping.
Every layer must declare `schema_version`; the value must equal both the active
bundle version and the value in every other contribution. A mismatch fails
before composition. Inheritance never migrates schema versions.

<a id="req-0623"></a>

**REQ-0623.** A layer is a schema-shaped fragment and need not be a complete
`root_class`. Unknown root fields and invalid values are errors in the layer
that writes them. Requiredness is deferred for root fields other than the entry
file's `output`, for direct members of the four keyed root collections, and for
every depth inside a `columns` member, because a later contribution may supply
their missing fields. The entry file must declare its complete, non-null
`output`; an inherited layer cannot choose the final artifact membership or
order.

<a id="req-0624"></a>

**REQ-0624.** The mapping key identifies an `input` member. Every member
of `intermediates`, `columns`, or `rows` must carry the respective `id`,
`name`, or `id` field. Two members of one layer must not share one identifier.

<a id="req-0625"></a>

**REQ-0625.** A non-null member field of `input`, `intermediates`, or `rows`
is complete at that field boundary. Its nested classes, mappings, lists,
registries, and scalar constraints validate normally; they are not partial
patches. A non-keyed root field supplied by a layer likewise validates as one
complete field value. A `columns` member field is instead a patch of the value
it composes onto. Every leaf it writes validates normally and only requiredness
is deferred, so a required leaf that no layer supplies fails on the resolved
specification under the rule that owns it.

<a id="req-0626"></a>

**REQ-0626.** [Schema language](../reference/schema-language.md) shorthand is expanded in every supplied non-null field before
composition. Equivalent long and short spellings therefore contribute the same
value.

### Composition

<a id="req-0627"></a>

**REQ-0627.** Composition merges the immediate fields of the root. A later field
that is absent leaves the accumulated field unchanged. A later non-null field
replaces the complete accumulated value unless the field is one of the keyed
collections below. There is no recursive merge inside a supplied root field
value.

<a id="req-0628"></a>

**REQ-0628.** The root fields compose as follows:

| Field | Composition |
|---|---|
| `schema_version` | Must be identical in every layer |
| `parents` | Traversal instruction; never contributed |
| `input` | Keyed by dataset ID |
| `intermediates` | Keyed by `id` |
| `columns` | Keyed by `name` |
| `rows` | Keyed by `id` |
| Every other root field | Complete field replacement |

<a id="req-0629"></a>

**REQ-0629.** Mappings and lists nested inside a replaced field are replaced
with
it. For example, later root `metadata`, `keys`, `output`, and `verifications`
replace their complete inherited values.

<a id="req-0630"></a>

**REQ-0630.** Members of a keyed collection compose by identifier. A new
identifier appends in contribution order. A matching identifier retains its
first position. A matching `input`, `intermediates`, or `rows` member then
merges the immediate fields of the member: an absent member field is inherited
and a present non-null member field replaces its complete value. A matching
`columns` member instead composes each present non-null field with the value it
inherits, by that field's declared kind:

| Kind | Composes by |
|---|---|
| Class | field by field, recursively |
| Mapping, `dict[K, V]` | key by key; each value composes by its own kind |
| Registry value | its one keyword |
| Every scalar and every list | replacement |

Composition descends only while both values are of the same kind. A written
value of a different kind replaces what it inherits, and a field the
accumulated member does not carry is taken whole. A registry value carries
exactly one keyword under [Expression evaluation](../operations/expressions.md), so two of them compose only when they name the
same keyword, and that keyword's payload then composes by its own declared
kind. Different keywords replace the value whole: composing across them would
build the two-keyword value [Expression evaluation](../operations/expressions.md) rejects, and naming a different operation is
how a layer says it derives the value differently. Every list replaces. A
column's `verifications` entries carry no identifier to compose by, and the
remaining lists are ordered arguments, such as `str_concat.sources`,
`order_by`, `first_available.sources`, and the `cut` breaks and labels, where
composing element by element would build a third argument list no layer wrote.
A schema default is materialized on the composed value rather than on each
contribution, so a later layer that never mentions a field cannot replace what
an earlier layer wrote there with this bundle's default. Thus a child may
change only `AVAL.label`, add one key to an inherited `AVAL.metadata`, or add
`missing` to an inherited `AVAL.derivation` without restating the expression,
while a child derivation naming a different expression keyword replaces the
whole derivation.

<a id="req-0631"></a>

**REQ-0631.** Dataset shorthand is expanded before datasets are matched. A bare
path becomes the long `dataset_class` form, after which matching dataset
declarations merge the immediate `path`, `types`, and `schema` fields by the
same rule.

### Clearing an optional field

<a id="req-0632"></a>

**REQ-0632.** YAML null at an immediate composition boundary clears an inherited
optional field. The marker is consumed and the field is absent from the
accumulated object. Clearing a required field, an identity field,
`schema_version`, or a field that has no inherited value is an error.

<a id="req-0633"></a>

**REQ-0633.** The marker applies only to an immediate root field or keyed-member
field, and composing a `columns` member does not move that boundary. Below it a
null keeps its [Schema language](../reference/schema-language.md) meaning at every depth composition reaches. In particular,
`derivation: {literal: null}` replaces the derivation with a literal missing
value; the replacement does not clear `derivation`. Likewise
`source.missing`, `mapping.missing`, and a result wrapper's `missing` declare the
missing value [Local handlers](../execution/handlers.md) substitutes, which [REQ-0344](../execution/handlers.md#req-0344) distinguishes from omitting the
field. A layer therefore clears a whole member field and restates what it
keeps; it does not remove one key of an inherited mapping or one field of an
inherited nested class.

<a id="req-0634"></a>

**REQ-0634.** There is no separate `remove`, `drop`, `output.add`, or
`output.remove` construct. Root fields are replaced explicitly, and unreachable
keyed declarations are pruned after composition.

### Path provenance

<a id="req-0635"></a>

**REQ-0635.** Every contributed value whose schema type is `path` is first
interpreted relative to the layer that writes that value, as its owning rule
requires. Composition must not silently reinterpret an inherited relative path
from the entry file's directory.

<a id="req-0636"></a>

**REQ-0636.** When the resolved specification is materialized, an inherited
relative path is rebased relative to the entry file without changing the
denoted local file. If the local platform cannot express that file relative
to the entry file, the canonical absolute local path is used. An absolute
contributed path is materialized exactly as it was written, because [Resource resolution](../storage/resources.md)
resolves it against the approved root it names and reads that written form.
`parents` paths are not materialized.

<a id="req-0637"></a>

**REQ-0637.** Rebasing states where a file is, not whether a run may read it. A
rebased `project_path` is accepted or rejected by [Resource resolution](../storage/resources.md) in its rebased form.

### Minimal resolved specification

<a id="req-0638"></a>

**REQ-0638.** After composition, the resolver removes declarations that cannot
affect the artifact or a declared assertion. Definitions do not make themselves
live. Reachability begins with:

- columns named by `output.columns`, `keys`, or `output.order_by`;
- columns read by dataset verifications;
- a column carrying its own column verification;
- every surviving row template, because declaring a row changes the artifact's
  rows; and
- a dataset named by `base`, including when a surviving row falls back to it.

<a id="req-0639"></a>

**REQ-0639.** The resolver then follows every semantic reference recursively.
This includes variables in derivations, row derivations, filters, grouping and
ordering, closed expression languages, string-template placeholders,
verifications, and record-lookup matching. A qualified variable makes its
dataset or record lookup live. A live record lookup makes its dataset and
matching inputs live. A live row makes its input dataset, filter inputs,
grouping inputs, and derivations needed for live columns live.

<a id="req-0640"></a>

**REQ-0640.** Dead entries are removed from `input`, `intermediates`, and
`columns`. Row-derivation entries targeting dead columns are removed with those
columns. Rows participate in reachability, but each final row declaration is
itself a root because it can add records; a resolver cannot discard one merely
because no other declaration names its `id`.

<a id="req-0641"></a>

**REQ-0641.** An inherited declaration excluded from `output.columns` may remain
as an internal column when a derivation or verification uses it. If no semantic
path reaches it, it and the declarations used only by it are removed.
Structural
validation still applies to every written layer field, but semantic name and
reference validation applies after pruning. An unresolved reference reachable
from a semantic root fails; one contained only in a dead declaration is
discarded with that declaration and is not an error.

### Deterministic column order

<a id="req-0642"></a>

**REQ-0642.** Keyed collection order initially follows first contribution: an
overridden member retains its position and a new member appends. Dataset,
record-lookup, and row order remains in that stable order.

<a id="req-0643"></a>

**REQ-0643.** After pruning, the resolver builds the column dependency graph
under [Execution lifecycle](../execution/lifecycle.md) and topologically orders the remaining columns. When more than one
column is ready, the column with the earliest initial collection position
comes first. This stable tie-break preserves `Common`, earlier-parent,
and child order for independent columns. An unknown dependency or dependency
cycle fails; sorting does not repair either one.

<a id="req-0644"></a>

**REQ-0644.** `output.columns` is not reordered. It alone defines artifact
membership and column order under [Artifact publication](../storage/publication.md).

### The resolved specification

<a id="req-0645"></a>

**REQ-0645.** The resolved specification:

contains no `parents` or null clearing markers;

<a id="req-0646"></a>

**REQ-0646.** uses the canonical long form of every [Schema language](../reference/schema-language.md) shorthand;

<a id="req-0647"></a>

**REQ-0647.** contains only reachable keyed declarations;

<a id="req-0648"></a>

**REQ-0648.** declares columns in the deterministic dependency order above;
  and

<a id="req-0649"></a>

**REQ-0649.** retains ordinary root and member fields in schema order when
  materialized.

<a id="req-0650"></a>

**REQ-0650.** Free-form mappings whose owning field was replaced whole retain
the order of the contribution that supplied them. The resolved YAML's
details such as indentation do not carry semantics; conformance compares its
YAML data tree.

<a id="req-0651"></a>

**REQ-0651.** Only after resolution does the implementation apply complete
`root_class` requiredness and every cross-field and semantic rule. A final
error
is reported under its owning rule. Its diagnostic identifies the entry
specification and the contributing file and field from which each implicated
value came.

## Error conditions

<a id="req-0652"></a>

**REQ-0652.** All inheritance failures occur in the `validation` phase:

<a id="req-0653"></a>

**REQ-0653.** A URL, URI, empty path, or non-local parent reference fails with
`invalid_parent_path` and reports the declaring file and `parents` entry.

<a id="req-0654"></a>

**REQ-0654.** A missing, unreadable, or non-file parent fails with
`parent_not_found` and reports the declaring file and path.

<a id="req-0655"></a>

**REQ-0655.** Reaching a file already on the active traversal path fails with
`inheritance_cycle` and reports the complete canonical path cycle.

<a id="req-0656"></a>

**REQ-0656.** A missing or inconsistent layer version fails with `schema_version_mismatch`
and
reports every implicated file and value.

<a id="req-0657"></a>

**REQ-0657.** An entry file that omits
`output` fails with `missing_entry_output`.

<a id="req-0658"></a>

**REQ-0658.** A malformed fragment
fails under [Schema language](../reference/schema-language.md) at its contributing file and field.

<a id="req-0659"></a>

**REQ-0659.** A duplicate
identifier within one layer fails with `duplicate_identifier`.

<a id="req-0660"></a>

**REQ-0660.** An
invalid null clearing marker fails with `invalid_clear` and reports the field
and contributing file.

<a id="req-0661"></a>

**REQ-0661.** An unknown reference, cycle, incomplete
final object, or other invalid final result fails under the rule that owns that
constraint, with contributing provenance included in the diagnostic.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [schema-inheritance](../../../benchmark/schema-inheritance/README.md).
- [negative-cyclic-parent](../../../benchmark/negative-cyclic-parent/README.md).
- [negative-inherited-output](../../../benchmark/negative-inherited-output/README.md).
- [negative-property-clear](../../../benchmark/negative-property-clear/README.md).
- [negative-version-mismatch](../../../benchmark/negative-version-mismatch/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Resolve inherited layers into one minimal, ordered specification. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
