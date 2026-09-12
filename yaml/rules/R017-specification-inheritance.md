---
id: R017
title: Specification Inheritance
status: normative
applies_to: [root.parents, root.schema_version, root]

---

# Specification inheritance

## Intent

Resolve reusable YAML layers into one complete, deterministic specification
before any data is read.

## Boundaries

This rule owns parent loading, graph traversal, composition, path provenance,
pruning, and the resolved specification. R006 owns YAML and schema validation.
R001 owns dependency inference and evaluation after this rule has ordered the
resolved columns. R002 and R015 own dataset and record-lookup references. R005
owns column coverage, output membership, and final identifier constraints. R009
owns verification behavior.

This rule does not execute a derivation, read a source dataset, or define what
an inherited field means. It composes declarations; the rule that already owns
each declaration applies to the resolved result.

## Terms

**R017-1.** The file requested for validation or execution is the **entry
file**. It and every file reached through `parents` are **layers**. A layer
contributes the root fields and keyed declarations it writes. The **resolved
specification** is the complete mapping produced from all contributions.

**R017-2.** A field is **present** when its mapping key is written, including
when its YAML value is null. Absence inherits; presence replaces or clears as
defined below.

**R017-3.** The resolver retains source provenance for every contributed value
while it works. Provenance is diagnostic state and is not a field of the
resolved specification.

## Parent references

**R017-4.** `parents` accepts one `path` or an ordered `list[path]`. A single
path is the R006 shorthand for a one-item list and is normalized before
traversal.

**R017-5.** A parent is a local YAML file. A relative path is resolved from the
directory of the layer that declares it. An absolute local path is permitted. A
URL or URI, including `file://`, is not a parent path. A path that is missing,
unreadable, or not a regular file is an error.

**R017-6.** The resolver canonicalizes an existing local path, including
symbolic links, before comparing file identity. Two spellings that reach the
same file identify one layer, not two.

## Linearization

**R017-7.** Starting at the entry file, visit each layer's normalized parents
from left to right, depth first, and then visit the layer itself. A layer
contributes once, at its first visit. Reaching a layer already on the active
traversal path is a cycle and fails; reaching one whose contribution is complete
skips it.

**R017-8.** For parents `A` then `B`, where both inherit `Common`, the
contribution order is therefore:

```text
Common -> A -> B -> entry
```

Later contributions have higher precedence. A difference between two parents is
resolved by their order; it is not a parent-conflict error. `parents` is
consumed during traversal and is absent from the resolved specification.

## Layer validation

**R017-9.** Every layer is parsed under R006 and must be a non-empty mapping. It
must declare `schema_version`; the value must equal both the active schema
bundle version and the value in every other contribution. A mismatch fails
before composition. Inheritance never migrates schema versions.

**R017-10.** A layer is a schema-shaped fragment and need not be a complete
`root_class`. Unknown root fields and invalid values are errors in the layer
that writes them. Requiredness is deferred for root fields other than the entry
file's `output`, and for direct members of the four keyed root collections,
because a later contribution may supply their missing fields. The entry file
must declare its complete, non-null `output`; an inherited layer cannot choose
the final artifact membership or order.

**R017-11.** A `datasets` member is identified by its mapping key. Every member
of `record_lookups`, `columns`, or `rows` must carry its respective `id`,
`name`, or `id` field. Two members of one layer must not share one identifier.

**R017-12.** A non-null field supplied inside a keyed member is a complete value
at that field boundary. Its nested classes, mappings, lists, registries, and
scalar constraints validate normally; they are not partial patches. A non-keyed
root field supplied by a layer likewise validates as one complete field value.

**R017-13.** R006 shorthand is expanded in every supplied non-null field before
composition. Equivalent long and short spellings therefore contribute the same
value.

## Shallow composition

**R017-14.** Composition merges the immediate fields of the root. A later field
that is absent leaves the accumulated field unchanged. A later non-null field
replaces the complete accumulated value unless the field is one of the keyed
collections below. There is no recursive merge inside a supplied field value.

**R017-15.** The root fields compose as follows:

| Field | Composition |
|---|---|
| `schema_version` | Must be identical in every layer |
| `parents` | Traversal instruction; never contributed |
| `datasets` | Keyed by dataset ID |
| `record_lookups` | Keyed by `id` |
| `columns` | Keyed by `name` |
| `rows` | Keyed by `id` |
| Every other root field | Complete field replacement |

**R017-16.** Mappings and lists nested inside a replaced field are replaced with
it. For example, later root `metadata`, `keys`, `output`, and `verifications`
replace their complete inherited values.

**R017-17.** Members of a keyed collection compose by their identifier. A new
identifier appends in contribution order. A matching identifier retains its
first position and merges the immediate fields of the member: an absent member
field is inherited and a present non-null member field replaces its complete
value. Thus a child may change only `AVAL.label`, while a child
`AVAL.derivation` replaces the whole derivation even when both derivations use
the same expression keyword. The same boundary applies to every member field,
including nested metadata, verification, type, lookup, and row-derivation
values.

**R017-18.** Dataset shorthand is expanded before datasets are matched. A bare
path becomes the long `dataset_class` form, after which matching dataset
declarations merge their immediate `path`, `types`, and `schema` fields by the
same rule.

## Clearing an optional field

**R017-19.** YAML null at an immediate composition boundary clears an inherited
optional field. The marker is consumed and the field is absent from the
accumulated object. Clearing a required field, an identity field,
`schema_version`, or a field that has no inherited value is an error.

**R017-20.** The marker applies only to an immediate root field or keyed-member
field. A null nested inside a supplied field value keeps its R006 meaning. In
particular, `derivation: {literal: null}` replaces the derivation with a literal
missing value; it does not clear `derivation`.

**R017-21.** There is no separate `remove`, `drop`, `output.add`, or
`output.remove` construct. Root fields are replaced explicitly, and unreachable
keyed declarations are pruned after composition.

## Path provenance

**R017-22.** Every contributed value whose schema type is `path` is first
interpreted relative to the layer that writes that value, as its owning rule
requires. Composition must not silently reinterpret an inherited relative path
from the entry file's directory.

**R017-23.** When the resolved specification is materialized, an inherited
relative path is rebased relative to the entry file without changing the local
file it denotes. If the local platform cannot express that file relative to the
entry file, the canonical absolute local path is used. An absolute contributed
path remains an absolute path. `parents` paths are not materialized.

**R017-24.** Rebasing states where a file is, not whether a run may read it. A
rebased `project_path` is accepted or rejected by R021 in its rebased form.

## Minimal resolved specification

**R017-25.** After composition, the resolver removes declarations that cannot
affect the artifact or a declared assertion. Definitions do not make themselves
live. Reachability begins with:

- columns named by `output.columns`, `keys`, or `output.order_by`;
- columns read by dataset verifications;
- a column carrying its own column verification;
- every surviving row template, because declaring a row changes the artifact's
  records; and
- a dataset named by `base`, including when a surviving row falls back to it.

**R017-26.** The resolver then follows every semantic reference recursively.
This includes variables in derivations, row derivations, filters, grouping and
ordering, closed expression languages, string-template placeholders,
verifications, and record-lookup matching. A qualified variable makes its
dataset or record lookup live. A live record lookup makes its dataset and
matching inputs live. A live row makes its driver dataset, filter inputs,
grouping inputs, and derivations needed for live columns live.

**R017-27.** Dead entries are removed from `datasets`, `record_lookups`, and
`columns`. Row-derivation entries targeting dead columns are removed with those
columns. Rows participate in reachability, but each final row declaration is
itself a root because it can add records; a resolver cannot discard one merely
because no other declaration names its `id`.

**R017-28.** An inherited declaration excluded from `output.columns` may remain
as an internal column when a derivation or verification uses it. If no semantic
path reaches it, it and the declarations used only by it are removed. Structural
validation still applies to every written layer field, but semantic name and
reference validation applies after pruning. An unresolved reference reachable
from a semantic root fails; one contained only in a dead declaration is
discarded with that declaration and is not an error.

## Deterministic column order

**R017-29.** Keyed collection order initially follows first contribution: an
overridden member retains its position and a new member appends. Dataset,
record-lookup, and row order remains in that stable order.

**R017-30.** After pruning, the resolver builds the column dependency graph
under R001 and topologically orders the remaining columns. When more than one
column is ready, the one with the earliest initial collection position comes
first. This stable tie-break preserves `Common`, earlier-parent, later-parent,
and child order for independent columns. An unknown dependency or dependency
cycle fails; sorting does not repair either one.

**R017-31.** `output.columns` is not reordered. It alone defines artifact
membership and column order under R005.

## The resolved specification

The resolved specification:

- **R017-32.** contains no `parents` or null clearing markers;
- **R017-33.** uses the canonical long form of every R006 shorthand;
- **R017-34.** contains only reachable keyed declarations;
- **R017-35.** declares columns in the deterministic dependency order above; and
- **R017-36.** retains ordinary root and member fields in schema order when
  materialized.

**R017-37.** Free-form mappings whose owning field was replaced whole retain the
order of the contribution that supplied them. The resolved YAML's presentation
details such as indentation do not carry semantics; conformance compares its
YAML data tree.

**R017-38.** Only after resolution does the implementation apply complete
`root_class` requiredness and every cross-field and semantic rule. A final error
is reported under its owning rule. Its diagnostic identifies the entry
specification and the contributing file and field from which each implicated
value came.

## Rationale

Inheritance composes declarations before any data is read, so every
specification resolves to one deterministic document: later contributions win by
position rather than by conflict, and `parents` order rather than a separate
merge rule decides every difference. Shallow composition with complete-field
replacement keeps each layer reviewable on its own, while null clearing handles
the one exception an optional field needs. Pruning keeps reuse cheap: a shared
layer can carry extra declarations without forcing them into every artifact, and
reference checks run after pruning so a dead declaration cannot fail a live one.
Path rebasing preserves what an inherited relative path denotes instead of
silently reinterpreting it from the entry directory.

## Errors

**R017-39.** All inheritance failures occur in the `validation` phase:

**R017-40.** A URL, URI, empty path, or non-local parent reference fails with
`invalid_parent_path` and reports the declaring file and `parents` entry.
**R017-41.** A missing, unreadable, or non-file parent fails with
`parent_not_found` and reports the declaring file and path. **R017-42.**
Reaching a file already on the active traversal path fails with
`inheritance_cycle` and reports the complete canonical path cycle. **R017-43.**
A missing or inconsistent layer version fails with `schema_version_mismatch` and
reports every implicated file and value. **R017-44.** An entry file that omits
`output` fails with `missing_entry_output`. **R017-45.** A malformed fragment
fails under R006 at its contributing file and field. **R017-46.** A duplicate
identifier within one layer fails with `duplicate_identifier`. **R017-47.** An
invalid null clearing marker fails with `invalid_clear` and reports the field
and contributing file. **R017-48.** An unknown reference, cycle, incomplete
final object, or other invalid final result fails under the rule that owns that
constraint, with contributing provenance included in the diagnostic.
