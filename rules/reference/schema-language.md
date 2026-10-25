# Schema language

The schema bundles under `yaml/` define the shape of every yamaa file: a
specification, project file, codelist file or function file. These rules fix
how a bundle is written and read: its documents and includes, the YAML every
document is read as, named types and registries, type expressions, shorthand
unions, the one spelling a written value takes, and descriptor keywords.

## Bundles and documents

<a id="schema-1"></a>

### SCHEMA-1. A schema bundle is one entry point and every document it includes, loaded whole

`schema_spec.yaml` is the specification-bundle entry point, and
`schema_project.yaml`, `schema_codelists.yaml` and `schema_function.yaml`
are the independent entry points of a project file, a codelist file and a
function file; a file's `type` selects its bundle ([SCHEMA-2](#schema-2)).
Every schema document is a YAML 1.2 mapping containing `version`, an optional
`includes` and declarations. `includes` is an ordered list of filenames
matching `schema_[a-z0-9_]+.yaml`, resolved relative to the including file.
The complete transitive bundle is loaded before names resolve, and every
document in a bundle declares the same version.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_registry_entries_and_closed_include_names_are_rejected`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::include_and_field_reuse_cycles_do_not_become_recursive_aliases`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::the_root_is_read_before_a_closure_is_chosen`, `rust/crates/yamaa-adapters/tests/declaration_v1_schema.rs::every_file_type_closure_is_admitted_with_valid_defaults_and_kinds`, `tools/tests/test_schema_v1.py`.

A document may be included by several entry points; declaration uniqueness
applies within each loaded bundle. Absolute paths, parent traversal, URLs,
missing files and include cycles are errors. Include order has no validation
or execution meaning. Except for registries, a declaration name may occur once
in the bundle. An unknown or duplicate type, registry, registry entry or schema
construct fails, and so does an invalid include or an inconsistent bundle
version.

A schema document also declares `kind` on its named value types and may
declare `since` on a class field ([SCHEMA-11](#schema-11)).

<a id="schema-2"></a>

### SCHEMA-2. A yamaa file declares its type and the oldest language version it needs, and an older engine refuses it

A specification, project file, codelist file or function file declares
`yamaa`, the oldest language version it needs, quoted as `"1.N"`, and
`type`: `spec`, `project`, `codelists` or `function`. An engine of language
version 1.M reads a file declaring 1.M or an older 1.x version, and admits a
feature only in a file declaring the version that introduced it or a later
one. A file declaring a version the engine does not read fails as
`unsupported_language_version`, and one whose `type` its call or referencing
field does not expect fails as `unexpected_file_type`.

Fails: `unsupported_language_version`, `unexpected_file_type`, `unknown_field`, `value_not_permitted`.
Evidence: `negative-version-mismatch`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::the_calling_function_refuses_an_unread_version_or_another_type`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::each_referencing_field_checks_the_file_it_names`, `rust/crates/yamaa-core/tests/preparation_dictionary_preparation.rs::a_fragment_with_an_unread_header_fails_at_the_include_entry_naming_it`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_listed_file_of_an_unread_version_and_one_of_another_type_fail_at_their_listings`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_codelist_file_where_a_function_file_belongs_fails_at_its_listing`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_listed_file_written_in_language_0_5_fails_at_its_listing`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_schema_link_refuses_a_producer_of_a_newer_version_at_the_link`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_schema_link_refuses_a_producer_of_another_type_at_the_link`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_schema_link_refuses_a_producer_that_declares_schema_version_at_the_link`, `rust/crates/yamaa/tests/issue_locations.rs::a_linked_producer_s_header_finding_locates_in_the_producer`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_listing_of_a_newer_version_is_refused_at_the_listing`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_listing_of_another_type_is_refused_at_the_listing`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_submission_refuses_a_listed_specification_of_a_newer_version`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::a_document_is_admitted_at_the_version_it_declares`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_public_check_reads_a_1_0_specification`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_public_check_refuses_a_newer_language_version_whatever_the_file_holds`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_public_check_refuses_a_specification_that_declares_schema_version`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_public_check_refuses_schema_version_with_yamaa_beside_it`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::schema_version_beside_a_1_0_header_is_refused_in_the_file_a_call_reads`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::schema_version_beside_a_1_0_header_is_refused_at_the_parent_that_names_it`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::schema_version_beside_a_1_0_header_is_refused_at_the_schema_link_that_names_it`, `rust/tests/host-fixtures/cases.json::check_refuses_a_specification_in_language_0_5`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_project_file_that_declares_schema_version_is_refused_as_language_0_5`, `rust/crates/yamaa-adapters/src/lint/tests.rs::a_named_file_that_declares_schema_version_refuses_the_call`, `rust/crates/yamaa/tests/language_header.rs`, `rust/crates/yamaa/tests/file_type.rs::a_file_that_declares_schema_version_has_no_type`, `rust/crates/yamaa/tests/file_type.rs::a_1_x_header_is_refused_as_a_call_reading_its_type_refuses_it`, `rust/crates/yamaa/tests/schema.rs::any_other_text_is_refused_as_an_unexpected_file_type`, `rust/crates/yamaa-core/src/language.rs::a_1_x_document_declares_its_type_and_a_0_5_document_none`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/file_origins.json::entry_version_finding_precedes_parent_and_study_authority`.

The header is read before the file is checked against any schema: `yamaa`
first, then `type`. A version is written `1.N`, with `N` a decimal number
without a leading zero. An unquoted `1.0` is a YAML float
([SCHEMA-4](#schema-4)) and fails as `invalid_field_type`, and a missing
`yamaa` or `type` fails as `missing_required_field`. A file whose version the
engine does not read is not checked further. A field or value newer than the
version a file declares fails in that file as `unknown_field` or
`value_not_permitted`, with context naming the version that introduced it;
each layer of a specification is admitted at its own version
([LAYER-4](../inheritance.md#layer-4)).

A file a call reads reports these at its own `yamaa` or `type`. Each
referencing field checks the header of the file it names: a project's
`functions`, `codelists` and section `specs`, a specification's `parent` and
`include` entries ([LAYER-4](../inheritance.md#layer-4)), and an input's
`schema`. A version the engine does not read, or a type the field does not
expect, is reported at that field; a malformed header is reported in the
named file. A dictionary file that `mapping.dict` names carries no header,
like a data file.

A file that declares `schema_version` is written in language version 0.5,
which no engine of language version 1.M reads. It fails as
`unsupported_language_version`, naming version `0.5`, wherever it is read,
with or without `yamaa` beside it: at its own `schema_version` when a call or
the lint names it, and at the field that names it when a `parent`, an
`include` entry, a project's `functions`, `codelists` or section `specs`, or
an input's `schema` reads it. A project file that declares it is refused
before any file it names is read.

A call that reads only the type a file declares, as the command line does
before it chooses what to run, reads the 1.x header alone. There a file that
declares `schema_version` declares no type and fails as
`unsupported_language_version` at `schema_version`, naming version `0.5`. A
file that declares one of the four types is refused as a call reading that
type refuses it; any other file's header is read in the same order, and a
`type` naming none of the four types fails as `unexpected_file_type`.

A call that takes a file type as its argument, as the call that gives a
type's JSON Schema does, takes it as a file writes it in `type`. Text naming
none of the four types fails as `unexpected_file_type`, expecting any of
them; the call reads no file, so the finding names no spec path.

<a id="schema-3"></a>

### SCHEMA-3. Documents reject duplicate keys, aliases, merge keys, explicit tags and unknown constructs

All schema documents must reject duplicate YAML keys, aliases, merge keys,
explicit tags and unknown schema constructs. Invalid YAML or a prohibited YAML
feature fails. Schema documents and the documents they validate use the ASCII
source boundary of [Values](../values.md), and a decoded string value
follows [Values](../values.md) even when written with ASCII escape
notation.

Fails: `invalid_yaml`.
Evidence: `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::complete_original_codec_reports_survive_schema_closure_admission`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::forbidden_metadata_reports_its_prefix_and_precedes_later_errors`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::merge_key_style_and_non_scalar_keys_are_checked`, `rust/crates/yamaa/tests/issue_locations.rs::a_parent_the_reader_refuses_locates_its_issue_in_that_parent`.

<a id="schema-4"></a>

### SCHEMA-4. Scalars resolve under the YAML 1.2 core schema, where only true and false spellings are Boolean

The schema bundle and every specification use the YAML 1.2 core schema. Only
`true`, `True`, `TRUE`, `false`, `False` and `FALSE` resolve to Boolean. Every
other alphabetic scalar, including `y`, `Y`, `n`, `N`, `yes`, `no`, `on` and
`off`, resolves to a string. The core schema has no timestamp resolver, so an
unquoted ISO-looking date or datetime is also a string. This holds without
requiring authors to quote values.

Evidence: `schema-non-finite`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::core_booleans_nulls_and_text_do_not_use_yaml11_resolvers`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::different_large_integer_keys_are_never_rounded_or_collapsed`.

Default parser settings do not satisfy this rule.
[Values](../values.md)'s non-finite normalization applies
immediately after core-schema scalar resolution. An integer scalar keeps its
exact decoded identity until schema and runtime range admission: overflow must
not silently round it or collapse distinct mapping keys. This does not expand
the signed 64-bit runtime integer range.

## Named types and registries

<a id="schema-5"></a>

### SCHEMA-5. A named type is a class, a value descriptor or a registry-backed type

A named type is a class, a value descriptor or a registry-backed type. A class
is an ordered list of one-entry mappings; each entry maps an allowed field name
to a descriptor or uses `fields_from` ([SCHEMA-6](#schema-6)). A value type is
a descriptor written directly as a mapping. A registry-backed type contains
exactly the `registry` keyword, naming a schema registry, and cannot also
declare `type` or descriptor constraints. Named-type references may cross
module boundaries, and recursive types are allowed.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::complete_bundle_resolves_forward_names_and_preserves_reused_default_origin`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::registry_contributions_merge_after_complete_include_loading`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_union_value_that_a_member_admits_and_a_nested_recursive_value_are_admitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_recursive_union_reports_the_nested_value_it_refuses_where_it_is_written`.

A specification value is finite and must eventually match a non-recursive
type.

```yaml
shared_fields:
    - filter: {type: predicate}
example_class:
    - name:
        type: str
        required: true
        description: Name of the example.
    - fields_from: shared_fields
    - values: {type: "list[str]", required: false}
identifier:
    type: str
    pattern: '^[A-Za-z_][A-Za-z0-9_]*$'
expression:
    registry: expressions
```

A field name is not a descriptor keyword and may match one. In
`- type: {type: column_type, required: true}` the outer name is the field
`type` of `column_class` and the inner `type` is the descriptor keyword. The
two uses are unrelated; [Values](../values.md) separates
the vocabularies.

<a id="schema-6"></a>

### SCHEMA-6. Class fields are closed, and `fields_from` copies another class's fields unchanged

Class fields are closed, and field order is descriptive with no execution
meaning. `fields_from` is reserved for class field reuse; its value must name a
class, not a value type or registry. It is resolved after the complete bundle
is loaded, including cross-module references. The copied fields keep
their descriptors, and a class cannot override one by repeating its name.
Unknown references, reuse cycles and duplicate field names after expansion are
errors.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::include_and_field_reuse_cycles_do_not_become_recursive_aliases`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_reused_fields_and_unknown_types_preserve_stage_order`, `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::original_sources_locations_reused_defaults_and_fresh_normalization_are_retained`.

<a id="schema-7"></a>

### SCHEMA-7. A registry maps keywords to payload shapes, and a registry value names exactly one keyword

A registry is a mapping from a permitted keyword to its payload shape, either a
class or a value descriptor, identified by a named type that references it with
`registry`. Multiple modules may contribute entries, and entry names are unique
across the complete bundle. A value matching a registry-backed type is a
one-entry mapping whose key exists in the referenced registry and whose value
matches that entry's payload shape.

Fails: `invalid_operation_count`, `unknown_operation`.
Evidence: `negative-literal-structure`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_registry_value_naming_no_keyword_declares_no_operation`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_registry_value_naming_a_keyword_the_registry_lacks_is_an_unknown_operation`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_registry_entries_and_closed_include_names_are_rejected`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_reused_fields_and_unknown_types_preserve_stage_order`.

```yaml
expressions:
    mapping:
        - source: {type: variable, required: true}
        - dict: {type: "dict[str, literal_value]", required: true}
```

Registry declaration order has no meaning. An unreferenced registry, an empty
registry, an unknown registry reference or a duplicate registry entry is an
error. A registry-backed value with zero or multiple keywords fails with
`invalid_operation_count`, and one with an unknown keyword with
`unknown_operation`.

<a id="schema-8"></a>

### SCHEMA-8. Two things in one naming list never share a name

A naming list is the set of names used in one place. Two things in one naming
list may not share a name. The rules name each place's list explicitly.

Evidence: `negative-lookup-id-collision`, `negative-source-self-reference`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/inheritance.json::a_reachable_qualifier_collision_is_rejected_in_a_flat_spec`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/inheritance.json::a_reachable_qualifier_collision_is_rejected_in_a_composed_spec`.

- a schema descriptor's `type`, a column's declared `type` and a value's
  runtime type belong to different naming lists
  ([VALUE-1](../values.md#value-1));
- datasets and record lookups share one naming list
  ([NAME-3](../names.md#name-3));
- a lookup `id` shares one naming list with dataset identifiers, other lookup
  ids and the output `domain` ([LOOKUP-3](../lookup.md#lookup-3));
- window names belong to a separate window naming list
  ([WINDOW-1](../windows.md#window-1)).

## Type expressions

<a id="schema-9"></a>

### SCHEMA-9. A type expression is a type name, a list or dict of type expressions, or a union

Built-in types are `str`, `int`, `float`, `bool`, `"null"`, `list` and `dict`.
A type expression is a type name, `list[` one type expression `]`, or `dict[`
two type expressions `]`; whitespace around nested expressions and the comma
is ignored, and a YAML sequence of type expressions is a union. Boolean values
are not integers. An `int` is accepted where `float` is expected, but a
`float` is not accepted where `int` is expected. An invalid type expression or
unresolved reference fails.

Evidence: `rust/crates/yamaa-core/tests/schema_references.rs::builtins_and_unresolved_names_are_syntax_not_bundle_resolution`, `rust/crates/yamaa-core/tests/schema_references.rs::malformed_types_and_string_encoded_unions_are_rejected`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_float_field_admits_an_integer_beyond_the_exact_range_of_a_float`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_float_field_refuses_a_boolean`.

```text
type_expression := type_name
                 | "list[" type_expression "]"
                 | "dict[" type_expression "," type_expression "]"
```

The quoted string `"null"` is a type name. An unquoted YAML null is a value.

`[` and `,` are structural characters inside a YAML flow mapping or flow
sequence, so a type expression containing either must be quoted wherever it is
written inside one. Block form imposes no such requirement, but the bundle
quotes a bracketed type expression in both:

```yaml
- keys: {type: "list[identifier]", required: true}
- dict: {type: ["dict[str, substitute_value]", project_path], required: true}
- order_by:
    type: "list[order_by_term]"
    required: false
    description: Terms ordering eligible records; declared with keep.
```

The quotes are YAML syntax and are not part of the type expression. Quoting
where YAML does not require it is a convention of the bundle; omitting it where
YAML does require it is a parse error.

<a id="schema-10"></a>

### SCHEMA-10. One union shape is shorthand, expanded to its canonical class while validating

A union of a non-class type `V` and a class declaring exactly one required
field whose type is `V` accepts either, and a bare `V` expands to that class
with the field set to it. An implementation expands shorthand while
validating, after the written value matches its union member, and a validated
document contains only the canonical class. A union of a type `T` and
`list[T]` is a bundle error, and no other union is shorthand.

Evidence: `schema-shorthand`, `rust/crates/yamaa-core/tests/schema_validation.rs::single_or_list_unions_are_bundle_errors_and_lists_normalize_as_written`, `rust/crates/yamaa-core/tests/schema_validation.rs::shorthand_checks_the_written_member_without_reinterpreting_generated_fields`.

The class is the canonical form. When a bare `V` expands, declared defaults
are applied to the remaining fields of the class and other optional fields
remain absent. This form also applies when `V` is registry-backed, as
`expression` is in `derivation`. Expansion happens after the written value has
been validated against the union member it matched, so a constraint on the
written form is checked before the value is expanded.

A field that takes several values is typed `list[T]` and is written as a list
even when it holds one value. No union offers the single value beside the
list, and no bare value is wrapped into one. A union matching neither shape,
such as `literal_value`, selects a member and expands nothing. A rule may say
where a shorthand applies and what the expanded value means, but the rule must
not define a different expansion.

<a id="schema-15"></a>

### SCHEMA-15. A written long form that holds only what its shorthand fills is refused

A value written through a shorthand union has one spelling. A written value
that matches the class of a shorthand union and holds nothing but what a bare
value of that union fills restates the shorthand and fails validation,
reporting its path, the restated field if there is one, and the shorthand to
write instead. A long form that writes any other field, even one restating a
default, keeps its meaning. The refusal applies to what an author wrote, never
to the canonical form a validation produces.

Fails: `redundant_long_form`.
Evidence: `negative-redundant-long-form`, `rust/crates/yamaa-adapters/tests/declaration_one_spelling.rs`, `rust/crates/yamaa-adapters/tests/declaration_one_spelling.rs::a_structured_source_holding_only_its_variable_restates_the_bare_read`, `rust/crates/yamaa-adapters/tests/declaration_v1_lowering.rs::a_check_that_declares_no_field_is_written_as_its_bare_keyword`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_dataset_mapping_holding_only_its_path_restates_the_path_shorthand`.

The shorthand unions of the specification bundle, and the long form each one
refuses, are:

| Written in | Shorthand | Refused long form |
|---|---|---|
| `aggregate` | `aggregate: E` | `aggregate: {expr: E}` |
| `flag` | `flag: P` | `flag: {condition: P}` |
| `str_template` | `str_template: T` | `str_template: {template: T}` |
| `odm` | `odm: D.I` | `odm: {item: D.I}` |
| `source` and a filtered source | `X` | `{variable: X}` |
| an order term | `X` | `{variable: X}` |
| `unique` | `unique: [A, B]` | `unique: {columns: [A, B]}` |
| an input dataset | `D: path.csv` | `D: {path: path.csv}` |
| a derivation | `E` | `{value: E}` |
| a submission comment, method or document reference | `T` | `{text: T}`, `{description: T}`, `{document: T}` |
| a column verification that declares no field | `- not_missing` | `- not_missing: {}` |

The refused long form of a bare keyword is the entry that keyword names with
nothing in it, and it reports the keyword as both the field and the shorthand.

The check runs on the document each author wrote: a standalone specification,
and each inheritance layer before composition. A composed, pruned or resolved
document holds the canonical class of every shorthand and is validated
without this check. The lint reports a written long form as the finding
`redundant_long_form`, and `lint --fix` writes its shorthand.

## Descriptors

<a id="schema-11"></a>

### SCHEMA-11. A descriptor uses only the closed descriptor keywords, each where it applies

Only these descriptor keywords are supported: `type`, `required`,
`description`, `pattern`, `min_length`, `size`, `values`, `default`, `kind`
and `since`. `type` is required and contains one type expression or a union.
Every other keyword is allowed only where the table below admits it. A
`default` is allowed only when `required` is false or absent, and it must
satisfy the same descriptor. An invalid descriptor keyword or default fails.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::findings_follow_keyword_type_required_description_constraint_order`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::invalid_types_and_required_flags_do_not_become_boolean_integer_coercions`, `rust/crates/yamaa-core/tests/schema_validation.rs::invalid_default_preparation_never_bypasses_normalization_admission`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::a_named_type_declares_a_kind_and_a_class_field_the_version_that_introduced_it`, `tools/tests/test_schema_v1.py::test_only_a_named_value_type_declares_a_known_kind`, `tools/tests/test_schema_v1.py::test_a_class_field_declares_the_version_that_introduced_it`.

| Keyword | Allowed | Meaning |
|---|---|---|
| `type` | every descriptor; required | one type expression or a union |
| `required` | a class field descriptor only | whether the field must be written; defaults to false |
| `description` | every descriptor; optional | a non-empty string documenting the field or value type; it has no effect on validation |
| `pattern` | `str` only | a regular expression; [Text](../text.md) owns its syntax, the engine that reads it, and what satisfying it means |
| `min_length` | `str` only | a minimum length, counted in [Values](../values.md) scalar values |
| `size` | `list` or `dict` only | an exact size |
| `values` | `str` only | permitted values, compared by [Values](../values.md) equality |
| `default` | when `required` is false or absent | a value satisfying the same descriptor |
| `kind` | a named value type only | the kind of the fields the type gives their type: `expression`, `name`, `identifier`, `value`, `path` or `structure`; [the 1.0 schema](../../yaml/README.md#the-10-schema) gives the kind a field takes |
| `since` | a class field descriptor only | the language version, quoted as `"1.N"`, that introduced the field; a file declaring an older version does not declare it ([SCHEMA-2](#schema-2)) |

Runtime semantics belong to the owning rule, which a description references.

<a id="schema-12"></a>

### SCHEMA-12. A value satisfies its type and every constraint its descriptor declares

When several constraints are present, all must pass. Defaults belong in the
schema, not only in prose. A value that fails its type or a constraint fails
validation, retaining the rejected value's declaring path and complete reason.

Fails: `invalid_field_type`, `value_not_permitted`, `pattern_mismatch`, `minimum_length`, `invalid_size`.
Evidence: `negative-str-case-unknown`, `negative-str-pad-width`, `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::captured_structural_projection_preserves_complete_ordered_constraint_outcome_and_authority`.

A value of the wrong type fails with `invalid_field_type`, a value outside
`values` with `value_not_permitted`, a string that does not satisfy `pattern`
with `pattern_mismatch`, a string shorter than `min_length` with
`minimum_length`, and a list or mapping of the wrong `size` with
`invalid_size`.

<a id="schema-13"></a>

### SCHEMA-13. A class value carries every required field and no undeclared field

A value of a class carries every field its class declares required and no
field the class does not declare. An undeclared or missing class field fails.

Fails: `unknown_field`, `missing_required_field`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_versionless_class_keeps_required_type_then_unknown_findings_in_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::classes_keep_schema_then_input_order_in_a_complete_document`.

Schema findings report an undeclared field as `unknown_field` and a missing
required field as `missing_required_field`.

<a id="schema-14"></a>

### SCHEMA-14. A descriptor written in flow form and in block form is the same descriptor

A descriptor may be written as a YAML flow mapping or in block form. The two
parse to the same mapping, and nothing in the schema language distinguishes
them. The bundle writes a class field descriptor in flow form without a
`description`, and in block form with a `description`; this is a convention
of how the bundle is written, not a validation requirement.

Evidence: `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::compact_flow_pairs_keep_nested_mapping_entries`, `rust/crates/yamaa-adapters/tests/declaration_v1_schema.rs::every_file_type_closure_is_admitted_with_valid_defaults_and_kinds`.

```yaml
example_class:
    - id: {type: str, required: true}
    - source:
        type: variable
        required: true
        description: Current-row value the record is matched on.
```

A document that mixes the forms differently is still valid.
