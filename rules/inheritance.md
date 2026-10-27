# Inheritance

A specification may inherit from a chain of parent layers, and each layer may
include reusable fragments. Composition turns the entry file and its layers
into one resolved specification before validation and execution.

## Layers

<a id="layer-1"></a>

### LAYER-1. A specification is its entry file composed with the chain of parents and the fragments it reaches

The file requested for validation or execution is the entry file. The entry
file, every file reached by following `parent` from it and every fragment a
layer lists under `include` are layers; each contributes the root fields and
keyed declarations it writes, and composition produces one resolved
specification. An entry file that names no parent and includes no fragment
is a composition of one layer and is resolved, pruned and validated on the
same terms.

Evidence: `schema-inheritance`, `schema-inherited-output`, `schema-inheritance-include`.

A field is present when its mapping key is written, including when its value
is null, the missing value. Absence inherits, and a written value replaces or
merges under [LAYER-5](#layer-5) to [LAYER-8](#layer-8); only a layer's
`clear` removes an inherited value ([LAYER-9](#layer-9)). The resolver keeps
the provenance of every contributed value, down to the leaf a layer wrote,
while it works. Provenance is diagnostic state, not a field of the resolved
specification; for a relative path it also records the declaring layer and
that layer's spelling, which [Resource resolution](reference/files.md#file-2)
resolves once.

A check may compose a fragment on a parent the caller names
([LAYER-4](#layer-4)): the fragment is then the entry file, and that parent
and every layer it reaches are layers that contribute before it.

<a id="layer-2"></a>

### LAYER-2. A parent or a fragment is one local YAML file resolved from the layer that names it

`parent` is an optional single path naming the layer this layer inherits from,
and `include` an optional list of paths naming the fragments it includes. A
relative path resolves from the directory of the layer that declares it, and
an absolute local path is allowed. A URL or URI, including `file://`, is not a
parent or fragment path. Two spellings that reach the same file, after
symbolic links are resolved, are one layer.

Fails: `invalid_parent_path`, `parent_not_found`.
Evidence: `negative-remote-parent`, `schema-inheritance-project-root`, `rust/crates/yamaa-application/tests/host_resources.rs::a_malformed_parent_spelling_is_refused_as_written_on_every_route`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_with_an_empty_segment_is_refused_as_written`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_with_a_trailing_slash_is_refused_as_written`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_on_a_network_share_root_is_refused_as_written`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_with_a_backslash_is_refused_as_written`, `rust/crates/yamaa-application/tests/host_resources.rs::a_linked_parent_is_refused_at_parent_on_every_route`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/file_origins.json::an_empty_parent_fails_schema_admission_before_it_is_resolved`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::an_empty_parent_fails_schema_admission_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::a_list_of_parents_carries_the_one_parent_hint`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::an_undeclared_parents_key_carries_the_one_parent_hint`.

A URL, URI or other non-local reference fails with `invalid_parent_path`,
reporting the declaring file and its `parent`. An empty `parent` fails schema
admission in its layer first: `parent` is a `path`, whose minimum length is
one, so it fails with `minimum_length`
([SCHEMA-12](reference/schema-language.md#schema-12)) before any parent is
resolved. A missing, unreadable or non-regular parent fails with
`parent_not_found`, reporting the declaring file and path. `parent` is a path,
so YAML null is not a parent: it fails as `invalid_field_type`
([LAYER-9](#layer-9)), and a layer without a parent omits the field. Nor is a
list: a layer names at most one parent, so a list written at `parent` fails as
`invalid_field_type` at `parent`, and its context adds a `hint` to write the
layers as a chain, one `parent` per file, and a fragment that several files
share under `include`. An undeclared `parents` key fails as
`unknown_field` ([SCHEMA-13](reference/schema-language.md#schema-13)) with the
same hint.

A non-empty local `parent` keeps the written form of every declared path
([FILE-4](reference/files.md#file-4), [FILE-5](reference/files.md#file-5)).
A `\`, an empty segment or a trailing separator in it, as in
`std//base.yaml`, `base.yaml/` or a network share's
`//server/share/base.yaml`, fails with `resource_path_not_normalized` at
`parent`, naming the path as written, before the parent is resolved.

A `parent` is walked from its anchor as every declared path is
([FILE-7](reference/files.md#file-7), [FILE-8](reference/files.md#file-8)):
a symbolic link below the anchor, as the layer's own name or as a directory on
its way, fails with `resource_path_symlink` at `parent`, naming the path as
written. The only symbolic links resolved before two spellings are compared
are those of the caller-selected directories that FILE-8 exempts.

An `include` entry is a declared path like an input's: its spelling is judged
when its layer is admitted, so a URI fails as `resource_path_uri_scheme` and a
`\`, an empty segment or a trailing separator as
`resource_path_not_normalized`, each at `include[i]`
([FILE-5](reference/files.md#file-5)). A fragment that is missing, is not a
regular file or is reached through a symbolic link fails as
`resource_path_missing`, `resource_path_not_regular_file` or
`resource_path_symlink` at `include[i]`, naming the path as written
([FILE-7](reference/files.md#file-7), [FILE-8](reference/files.md#file-8)).
An empty list includes nothing.

<a id="layer-3"></a>

### LAYER-3. Layers contribute from the root ancestor to the entry, each after its fragments, later layers taking precedence

Starting at the entry file, each layer's `parent` names the next layer up the
chain, which ends at a layer without a parent. Layers contribute from that
root ancestor down to the entry file, each just after the fragments its
`include` lists, in list order, and a later contribution takes precedence over
an earlier one. Reaching a layer that is still being composed is a cycle, and
an `include` entry that reaches any other layer already reached fails.

Fails: `inheritance_cycle`, `duplicate_layer`.
Evidence: `schema-inheritance`, `negative-cyclic-parent`, `schema-inheritance-include`, `negative-include-cycle`, `negative-include-twice`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_given_parent_that_is_the_fragment_itself_is_a_cycle_at_parent`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::fragments_contribute_after_the_parent_chain_and_before_their_layer`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::an_include_that_reaches_a_layer_still_composing_is_a_cycle`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::an_include_that_reaches_a_layer_already_composed_repeats_it`.

For a study entry whose parent is a compound layer whose parent is an
organization layer, the contribution order is
`organization -> compound -> study`. When the compound layer includes
`duration.yaml` and the study includes `severity.yaml` and then
`period.yaml`, it is
`organization -> duration -> compound -> severity -> period -> study`: at
every layer its parent chain, then its fragments left to right, then its own
fields. A difference between two layers is resolved by this order, not by a
conflict error.

A layer is admitted under [LAYER-4](#layer-4) before its parent or its
fragments are read. The chain is followed to its root first, and the
fragments are then read in contribution order, each admitted before the next
is read. At most 8192 links are followed, counting each `parent` and each
`include` entry. `parent` and `include` are consumed by the traversal and are
absent from the resolved specification.

A cycle is a `parent` that reaches a layer already on the chain, or an
`include` entry that reaches the layer that lists it or a layer below it on
the chain. It reports the complete canonical path of the cycle, from the layer
reached twice back to itself, at the `parent` or `include[i]` that closes it,
and whether the chain returned to the entry file or to another layer on it.
An `include` entry that reaches any other layer already reached, such as the
layer's own parent or a fragment that another layer, or an earlier entry of
the same list, includes, fails as `duplicate_layer` at that `include[i]`,
naming the path as written and the layer it reaches. A file contributes once
or not at all.

<a id="layer-4"></a>

### LAYER-4. Every layer is a valid partial specification of the language version it declares

Each layer must be a non-empty mapping that declares `yamaa` and
`type: spec` ([SCHEMA-2](reference/schema-language.md#schema-2)), and is
admitted against the language version it declares; the resolved specification
records the newest version any layer declares. A layer need not be complete:
requiredness is checked on the resolved specification, but unknown fields and
invalid values fail in the layer that writes them. Two members of one keyed
collection in one layer must not share an identifier. A fragment declares
neither `parent` nor `include`.

Fails: `unsupported_language_version`, `unexpected_file_type`, `duplicate_identifier`, `prohibited_construct`, `value_not_permitted`.
Evidence: `negative-version-mismatch`, `negative-include-parent`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_fragment_declares_neither_parent_nor_include`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_parent_a_caller_names_contributes_before_the_fragment`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_fragment_that_names_a_layer_refuses_the_given_parent_before_it_is_read`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_given_parent_with_an_unread_header_fails_in_that_file`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_fragment_with_an_unread_header_fails_at_the_include_entry_naming_it`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::each_layer_is_admitted_at_its_own_version_and_the_chain_needs_the_newest`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::a_parent_of_a_newer_version_fails_at_parent`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::a_parent_of_another_type_fails_at_parent`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::a_parent_without_a_header_fails_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::a_layer_is_a_fragment_whose_written_fields_are_checked`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::a_single_layer_is_complete_at_its_version`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::a_parent_that_declares_schema_version_fails_at_parent`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/language_header.json::schema_version_beside_a_1_0_header_is_refused_at_the_parent_that_names_it`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_column_without_a_name_misses_it_at_its_index`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::an_empty_column_member_misses_its_name_at_its_index`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::an_intermediate_without_an_id_misses_it_at_its_index`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_row_template_without_an_id_misses_it_at_its_index`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_child_column_without_a_name_misses_it_in_the_child`, `rust/tests/host-fixtures/cases.json::a_grandparent_of_another_type_at_the_parent_that_names_it`, `rust/tests/host-fixtures/cases.json::a_fragment_of_another_type_at_the_ancestor_that_includes_it`.

Requiredness is deferred for every root field and at every depth inside a
member of `input`, `intermediates`, `columns`, `rows` and `windows`. An entry
file may therefore omit `output` and inherit it. The mapping key identifies an
`input` or `windows` member; a member of `intermediates`, `columns` or `rows`
carries its `id`, `name` or `id` in the layer that writes it, and one without
it fails as `missing_required_field` under
[SCHEMA-13](reference/schema-language.md#schema-13). A `copy` entry carries no
`name`: it expands before the layers compose
([SPEC-9](specification.md#spec-9)), so it is complete where it is written. A
layer whose version the engine does not read fails before composition,
reporting the file and its version, at the
entry's `yamaa` or at the `parent` or `include[i]` naming the layer. A layer
that declares a type other than `spec` fails as `unexpected_file_type`, at the
entry's `type` or at that `parent` or `include[i]`. At a `parent` or
`include[i]`, either finding also names the layer that writes that field,
which may be a layer above the entry. A fragment composes on the
layers before it, so a fragment that declares `parent` or `include` fails as
`prohibited_construct` at that field of the fragment, its context naming the
construct, the fragment and the entry, before anything it names is read.
Layers may declare different versions, each admitted at its own, and a
`clear:` path is checked against the version the resolved specification
records. Inheritance never migrates versions. A malformed
layer fails under
[Schema language](reference/schema-language.md) at its contributing file and
field. [Schema language](reference/schema-language.md) shorthand is expanded
in every supplied non-null field before composition, so equivalent spellings
contribute the same value. A long form that only restates its shorthand fails
in the layer that writes it under
[SCHEMA-15](reference/schema-language.md#schema-15); the composed document
holds the expanded classes and is not checked again.

A check may compose a fragment on a parent the caller names, a
specification it gives as the `parent` argument. The fragment composes as it
would in a file that names that specification as its `parent`, lists the
fragment under `include` and writes nothing else: it is the entry, and the
given parent's layers contribute before it ([LAYER-3](#layer-3)). The given
parent is a path the caller passes ([FILE-1](reference/files.md#file-1)),
and a header it declares that the engine does not read fails at its own
`yamaa` or `type`, as the entry's does. An entry that declares `parent` or
`include` is no fragment: the given parent is refused with
`value_not_permitted` at `parent`, the argument, its context naming the
construct and its `value` the path as the caller gave it, before it is
read. A given parent that is the entry itself is an
`inheritance_cycle` at `parent`.

## Composition

<a id="layer-5"></a>

### LAYER-5. Root fields replace whole; keyed collections compose member by member

A later layer's root field replaces the accumulated value whole, including
every mapping and list nested in it. `input`, `intermediates`, `columns`,
`rows` and `windows` instead compose member by member: a new identifier
appends in contribution order, and a matching identifier keeps its first
position and merges with the member it inherits under [LAYER-6](#layer-6).

Evidence: `schema-inherited-output`, `schema-inherited-input`.

| Root field | Composes |
|---|---|
| `yamaa` | the newest version any layer declares ([LAYER-4](#layer-4)) |
| `type` | `spec` in every layer |
| `parent` | traversal instruction; never contributed |
| `include` | traversal instruction; never contributed |
| `clear` | applied before the layer's own fields; never contributed ([LAYER-9](#layer-9)) |
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
inherited `path` and `schema` without restating them. A `copy` entry expands
to the columns it declares before its layer composes
([SPEC-9](specification.md#spec-9)), so each copied column composes by its
`name`, and a later layer's column of that name merges with it.

<a id="layer-6"></a>

### LAYER-6. A matching member merges recursively: classes by field, mappings by key, scalars and lists by replacement

When a later layer writes a member whose identifier is already present, each
field it writes composes with the value it inherits: a class field by field
and a mapping key by key, each value by its own kind; a registry value only
when both name the same keyword, and otherwise whole; every scalar and every
list by replacement. A value of a different kind replaces what it inherits,
and a field the member does not inherit is taken whole.

Evidence: `schema-column-composition`, `schema-inherited-input`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::recursive_patches_keep_positions_merge_mappings_replace_lists_and_defer_defaults`.

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

A layer that restates an inherited mapping merges with it all the same, so an
entry the layer leaves out comes back from its parent. Clearing the mapping
in the same layer replaces it instead ([LAYER-9](#layer-9)):

```yaml
yamaa: "1.0"
type: spec
parent: scores-base.yaml
clear:
  - rows.scores.derivations

rows:
  - id: scores
    derivations:
      USUBJID: SC.USUBJID
      VISIT: SC.VISIT
      AVAL: SC.SCORE
```

Without the `clear` entry, the `scores` template keeps every other derivation
`scores-base.yaml` gives it, such as one for `NOTE`, and the lint reports the
restatement as `partial_restatement`. A named window composes the same way,
and since `clear` never names the window itself, a layer clears each setting
it leaves out by that setting's path ([LAYER-8](#layer-8)).

<a id="layer-7"></a>

### LAYER-7. Requiredness and defaults apply to the composed member, never to one contribution

Every leaf a layer writes inside a member validates in that layer, but
requiredness and schema defaults apply to the composed member. A required
field that no layer supplies fails on the resolved specification, and a later
layer that never mentions a field cannot replace an earlier value with the
field's default.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::complete_composition_materializes_defaults_once_and_keeps_written_provenance`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::keyed_member_fragments_defer_nested_requiredness_but_root_values_do_not`, `rust/crates/yamaa-application/tests/declaration_layer_composition.rs::a_cleared_field_takes_its_default_again_and_its_path_its_base`.

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

Evidence: `rust/crates/yamaa-application/tests/declaration_layer_composition.rs::a_named_window_composes_by_name_and_every_reference_uses_the_final_definition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_named_window_merged_by_name_saves_every_reference_by_the_final_definition`.

A window name is not a variable or dataset dependency. A setting the composed
definition omits takes its ordinary window meaning. A layer clears the
inherited collection by listing `windows` in `clear` under
[LAYER-9](#layer-9), and an inherited setting by listing its path; an
individual definition, a keyed member, is never cleared or null. A named reference and an inline window are different kinds, so replacing
one with the other replaces the complete `window` value.

<a id="layer-9"></a>

### LAYER-9. YAML null is the missing value, and a layer clears an inherited field only by listing it in `clear`

YAML null is the missing value wherever a value or an expression may be
missing, and it never removes anything. A layer clears an inherited value only
by listing its spec path in the root field `clear`; composition removes the
value before the layer's own fields compose, and a field with a schema default
takes it again. A path that names nothing the layer inherits, a required or
identity field, `yamaa`, `type` or a keyed member fails as `invalid_clear`.

Fails: `invalid_clear`.
Evidence: `schema-inheritance-clear`, `negative-property-clear`, `rust/crates/yamaa-application/tests/declaration_layer_composition.rs::a_clear_removes_what_the_layer_inherits_before_its_own_fields_compose`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::each_clear_entry_naming_what_cannot_be_cleared_fails_at_its_index`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_clear_of_a_keyed_member_fails_at_its_entry`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_clear_of_an_operation_keyword_without_its_field_fails_at_its_entry`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_clear_of_a_schema_default_fails_at_its_entry`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_clear_in_a_layer_without_a_parent_fails_in_that_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_clear_path_outside_the_notation_or_beside_its_prefix_fails_at_its_entry`, `rust/crates/yamaa-application/tests/declaration_layer_composition.rs::yaml_null_is_the_missing_value_and_never_removes_anything`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::yaml_null_as_a_column_label_is_an_invalid_field_type_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::yaml_null_as_a_key_is_an_invalid_field_type_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::yaml_null_as_the_parent_is_an_invalid_field_type_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::yaml_null_as_a_metadata_value_is_an_invalid_field_type_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::yaml_null_as_the_output_path_is_an_invalid_field_type_in_its_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::the_converted_negative_property_clear_fails_as_its_source_at_clear_0`, `rust/crates/yamaa-core/src/schema/spec_path.rs::a_clear_path_names_keys_by_segment_or_quoted_text`.

`clear` is a root field, written after `parent`, that lists spec paths:

```yaml
clear:
  - columns.AVAL.verifications
  - columns.DCSREAS.derivation.mapping.dict.PD
  - columns.RACEN.derivation.mapping.dict['NOT REPORTED']
  - rows.collected-occurrence.filter
  - intermediates.DEATHEV.absent
  - input.ADSL.require_match
  - metadata.scope
```

A path is written in the notation diagnostics use: a root field, then `.key`
for a key made of letters, digits, `_` and `-`, and `['key']` for any other
key, with `'` written twice. Keyed members are named by identity, as
[LAYER-5](#layer-5) composes them: `input` and `windows` members by key,
`columns` by `name`, and `intermediates` and `rows` by `id`. A column a `copy`
entry declares is named as any column is: `columns.AGE.label` clears the label
a copied `AGE` takes from its producer ([SPEC-9](specification.md#spec-9)). A list replaces
whole ([LAYER-6](#layer-6)), so a path never holds a list index, and `$`, the
root, appears only in diagnostics. A value written through a shorthand is named
through it, as its layer wrote it: `columns.DCSREAS.derivation.mapping.dict.PD`
names an entry of a mapping derivation written bare.

Each layer's clears apply to the value it inherits before its own fields
compose, so clearing a mapping and writing it in one layer replaces the mapping
instead of merging with it. A fragment inherits what the layers before it
compose: its parent chain's and those of the fragments listed before it. A
fragment's clear may therefore name a value one layer that includes it
inherits and another does not, and fails as `invalid_clear` there. A cleared
field with a schema default takes the default again under
[LAYER-7](#layer-7), and a value that came from a default is not inherited. A
path that names a value the layer does not inherit, a required field, the
identity field of a keyed member, `yamaa`, `type`, a keyed member itself or a
registry keyword without its owning field fails as `invalid_clear`, and so
does a path outside the notation or a path written beside one of its prefixes
in the same list. A finding about a clear is
reported at `clear[i]`, its context naming the field the path ends at.
Composition consumes `clear` as it consumes `parent` and `include`, and the
resolved specification holds none of them ([LAYER-14](#layer-14)).

A null a layer writes is a value of its field: a handler, flag value, mapping
entry or function argument written as YAML null substitutes the missing value
([COLUMN-10](columns.md#column-10)), and `derivation: null` derives missing.
Null is admitted only where a value or an expression may be missing: in a
structure, identifier, path, name, enumeration, Boolean or number field it
fails as `invalid_field_type`, and a keyed member, a list item and a root
collection are never null.

<a id="layer-10"></a>

### LAYER-10. A path keeps its declaring layer's base and authored spelling

A contributed value whose schema type is `path` is interpreted at the base its
owning rule selects: the declaring layer by default, the entry file when
`relative_to: entry` selects it. Composition keeps the authored spelling and
provenance. A materialized resolved specification writes an inherited relative
path relative to the entry file, preserving the location it names.

Fails: `parent_on_another_share`.
Evidence: `schema-inheritance-project-root`, `rust/crates/yamaa-adapters/src/file_metadata.rs::rebase_respells_layers_on_the_entry_root_relative_to_the_entry`, `rust/crates/yamaa-adapters/src/file_metadata.rs::rebase_writes_a_location_on_another_root_whole_and_never_climbs_to_it`, `rust/crates/yamaa-adapters/src/file_metadata.rs::a_layer_on_another_share_is_refused_with_its_path_and_both_roots`, `rust/crates/yamaa-application/tests/host_resources.rs::a_parent_on_another_drive_reads_its_input_beside_itself`, `rust/crates/yamaa-application/tests/host_resources.rs::a_parent_on_another_share_is_refused_rather_than_read_from_the_entry_root`.

A fragment is a declaring layer: a relative path it writes is interpreted
beside the fragment, whichever layer includes it.

If the platform cannot express that location relative to the entry file, the
canonical absolute local path is written. A location beside a layer on a
network share that the entry is not on has neither spelling
([FILE-4](reference/files.md#file-4)). Composition then fails with
`parent_on_another_share`, reported at `parent` and naming that layer, the
path it wrote, and the roots of the layer and the entry. A fragment on such a
share fails the same way, naming the fragment, the path it wrote and both
roots; the finding keeps the spec path `parent`, since the rebase that refuses
it does not know which `include` entry named the fragment. An absolute
contributed path is materialized exactly as written. Neither a `parent` path
nor an `include` entry is materialized.
Rebasing states where a file is, not whether a run may read it:
[File resolution](reference/files.md) accepts or rejects the rebased
path at the location it names, and a missing file fails there without retrying
another spelling.

## The resolved specification

<a id="layer-11"></a>

### LAYER-11. Declarations that cannot affect the artifact or an assertion are pruned

After composition, the resolver keeps only declarations reachable from the
semantic roots and removes the rest, whether or not the entry file declares a
`parent` or includes a fragment. The roots are the columns named by
`output.columns`, `keys` or `output.order_by`, the columns dataset
verifications read, every column or intermediate that carries its own
verification, every row template, and the dataset `base` names.

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

Evidence: `rust/crates/yamaa-application/tests/declaration_layer_composition.rs::an_entry_that_omits_output_resolves_with_the_output_its_parent_contributes`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::an_input_only_an_unread_lookup_reads_is_pruned_with_its_missing_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::a_resolved_specification_no_layer_gives_output_misses_it_at_the_root`.

A declaration left out of `output.columns` remains as an internal column when a
derivation or verification uses it. A resolved specification to which no layer
contributed `output` fails as `missing_required_field` at `output`
([SCHEMA-13](reference/schema-language.md#schema-13)); an entry file that omits
`output` is not itself an error.

<a id="layer-13"></a>

### LAYER-13. A layered specification's resolved columns follow dependency order with a stable tie-break

When a specification declares a `parent` or includes a fragment, each
resolved column comes after every column its derivation reads, after pruning;
when more than one column can come next, the one with the earliest
contribution position comes first, which keeps an ancestor's independent
columns before its descendants'. A specification that does neither keeps its
declaration order, which [COLUMN-5](columns.md#column-5) requires to be a
dependency order. `output.columns` is never reordered.

Evidence: `schema-inheritance`, `rust/crates/yamaa-core/tests/schema_references.rs::declared_order_and_unordered_input_are_kept`, `rust/crates/yamaa-core/tests/schema_references.rs::prunes_dead_sources_follows_count_and_stably_orders_without_changing_output`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::ordering_reads_a_row_template_s_own_driver_without_a_join`.

The columns a derivation reads are those [COLUMN-3](columns.md#column-3)
collects from it, the columns of an input's declared `key` included for each
implicit join and each qualified aggregate without `key` that reads that
input. A row template's qualified read of its own driver reads the row's own
record ([LOOKUP-1](lookup.md#lookup-1)), so it adds no read of the driver's
key columns.

A derivation that reads an undeclared column, or that reads itself directly or
through other columns, fails under the
[Column derivation](columns.md); ordering repairs neither.
`output.columns` alone defines artifact membership and column order under
[Output](output.md#output-1).

<a id="layer-14"></a>

### LAYER-14. The resolved specification is canonical

The resolved specification contains no `parent`, no `include` and no
`clear`, uses the long form of every shorthand, holds only reachable keyed
declarations, lists columns in the order of [LAYER-13](#layer-13), and keeps
root and member fields in schema order. Conformance compares its YAML data
tree, not its layout.

Evidence: `rust/crates/yamaa-application/tests/declaration_layer_composition.rs::a_clear_removes_what_the_layer_inherits_before_its_own_fields_compose`.

A free-form mapping whose owning field was replaced whole keeps the order of
the contribution that supplied it. Composition consumes a layer's `clear` as
it consumes `parent` and `include` ([LAYER-9](#layer-9)), and `yamaa`
records the newest version any layer declares, a fragment's included.
