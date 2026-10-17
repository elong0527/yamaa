# Schema language

The schema bundle under `yaml/` defines the shape of every specification and
project environment. These rules fix how the bundle is written and read: its
documents and includes, the YAML every document is read as, named types and
registries, type expressions, shorthand unions and descriptor keywords.

## Bundles and documents

<a id="schema-1"></a>

### SCHEMA-1. A schema bundle is one entry point and every document it includes, loaded whole

`schema.yaml` is the specification-bundle entry point, and
`schema_environment.yaml` is the independent project-environment-bundle entry
point of [Project functions](../profiles/functions.md). Every schema document
is a YAML 1.2 mapping containing `version`, an optional `includes` and
declarations. `includes` is an ordered list of filenames matching
`schema_[a-z0-9_]+.yaml`, resolved relative to the including file. The
complete transitive bundle is loaded before names resolve, and every document
in a bundle declares the same version.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_registry_entries_and_closed_include_names_are_rejected`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::include_and_field_reuse_cycles_do_not_become_recursive_aliases`, `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::shipped_closure_retains_every_authoritative_byte_and_admits_without_a_directory`.
Replaces: REQ-0242, REQ-0243, REQ-0244, REQ-0282.

A document may be included by both entry points; declaration uniqueness
applies within each loaded bundle. Absolute paths, parent traversal, URLs,
missing files and include cycles are errors. Include order has no validation
or execution meaning. Except for registries, a declaration name may occur once
in the bundle. An unknown or duplicate type, registry, registry entry or schema
construct fails.

<a id="schema-2"></a>

### SCHEMA-2. A specification or environment declares in `schema_version` the bundle version it is written against

A specification or project environment declares its entry-point bundle
version in `schema_version`, and the declared version must equal the bundle
version. Any other version is an error, and the document is not validated
against the bundle. An invalid include or an inconsistent bundle version also
fails.

Fails: `schema_version_mismatch`.
Evidence: `negative-version-mismatch`, `rust/crates/yamaa-core/tests/schema_references.rs::version_precedes_field_findings_and_classes_keep_schema_then_input_order`, `rust/crates/yamaa-adapters/tests/file_diagnostic_origins.rs::entry_version_finding_precedes_parent_and_study_authority`.
Replaces: REQ-0245, REQ-0281.

A specification whose `schema_version` does not equal the bundle version fails
with `schema_version_mismatch`.

<a id="schema-3"></a>

### SCHEMA-3. Documents reject duplicate keys, aliases, merge keys, explicit tags and unknown constructs

All schema documents must reject duplicate YAML keys, aliases, merge keys,
explicit tags and unknown schema constructs. Invalid YAML or a prohibited YAML
feature fails. Schema documents and the documents they validate use the ASCII
source boundary of [Text values](../values/text.md), and a decoded string value
follows [Text values](../values/text.md) even when written with ASCII escape
notation.

Fails: `invalid_yaml`.
Evidence: `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::complete_original_codec_reports_survive_schema_closure_admission`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::forbidden_metadata_reports_its_prefix_and_precedes_later_errors`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::merge_key_style_and_non_scalar_keys_are_checked`.
Replaces: REQ-0246, REQ-0247, REQ-0280.

<a id="schema-4"></a>

### SCHEMA-4. Scalars resolve under the YAML 1.2 core schema, where only true and false spellings are Boolean

The schema bundle and every specification use the YAML 1.2 core schema. Only
`true`, `True`, `TRUE`, `false`, `False` and `FALSE` resolve to Boolean. Every
other alphabetic scalar, including `y`, `Y`, `n`, `N`, `yes`, `no`, `on` and
`off`, resolves to a string. The core schema has no timestamp resolver, so an
unquoted ISO-looking date or datetime is also a string. This holds without
requiring authors to quote values.

Evidence: `schema-non-finite`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::core_booleans_nulls_and_text_do_not_use_yaml11_resolvers`, `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::different_large_integer_keys_are_never_rounded_or_collapsed`.
Replaces: REQ-0248, REQ-0249.

Default parser settings do not satisfy this rule.
[Types and conversion](../values/types.md)'s non-finite normalization applies
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

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::complete_bundle_resolves_forward_names_and_preserves_reused_default_origin`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::registry_contributions_merge_after_complete_include_loading`, `rust/crates/yamaa-core/tests/schema_references.rs::union_selects_success_then_matching_outer_type_and_recursion_is_value_local`.
Replaces: REQ-0250, REQ-0251, REQ-0253, REQ-0254, REQ-0255, REQ-0256.

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
two uses are unrelated; [Types and conversion](../values/types.md) separates
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
Replaces: REQ-0252.

<a id="schema-7"></a>

### SCHEMA-7. A registry maps keywords to payload shapes, and a registry value names exactly one keyword

A registry is a mapping from a permitted keyword to its payload shape, either a
class or a value descriptor, identified by a named type that references it with
`registry`. Multiple modules may contribute entries, and entry names are unique
across the complete bundle. A value matching a registry-backed type is a
one-entry mapping whose key exists in the referenced registry and whose value
matches that entry's payload shape.

Fails: `invalid_operation_count`, `unknown_operation`, `model_contract_mismatch`.
Evidence: `rust/crates/yamaa-core/tests/schema_references.rs::registry_and_bare_derivation_diagnostics_preserve_paths_and_context`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_registry_entries_and_closed_include_names_are_rejected`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::duplicate_reused_fields_and_unknown_types_preserve_stage_order`.
Replaces: REQ-0257, REQ-0258, REQ-0259, REQ-0260, REQ-0286.

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
`unknown_operation`. Final normalized model admission reports
`model_contract_mismatch` for operation cardinality.

<a id="schema-8"></a>

### SCHEMA-8. Two things in one naming list never share a name

A naming list is the set of names used in one place. Two things in one naming
list may not share a name. The rules name each place's list explicitly.

Evidence: `negative-lookup-id-collision`, `negative-source-self-reference`, `rust/crates/yamaa-adapters/tests/execution_declarative_self.rs::a_reachable_qualifier_collision_is_rejected_in_flat_and_composed_specs`.

- a schema descriptor's `type`, a column's declared `type` and a value's
  runtime type belong to different naming lists
  ([REQ-0001](../values/types.md#req-0001));
- datasets and record lookups share one naming list
  ([REQ-0084](../specification/binding.md#req-0084));
- a lookup `id` shares one naming list with dataset identifiers, other lookup
  ids and the output `domain` ([REQ-0113](../operations/lookup.md#req-0113));
- window names belong to a separate window naming list
  ([REQ-1251](../operations/windows.md#req-1251)).

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

Evidence: `rust/crates/yamaa-core/tests/schema_references.rs::builtins_and_unresolved_names_are_syntax_not_bundle_resolution`, `rust/crates/yamaa-core/tests/schema_references.rs::malformed_types_and_string_encoded_unions_are_rejected`, `rust/crates/yamaa-core/tests/schema_references.rs::constraints_keep_order_unicode_lengths_and_alias_requirements_after_type_success`.
Replaces: REQ-0261, REQ-0262, REQ-0263, REQ-0283.

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
- parents: {type: [path, "list[path]"], required: false}
- order_by:
    type: "list[order_by_term]"
    required: false
    description: Terms ordering eligible records; declared with keep.
```

The quotes are YAML syntax and are not part of the type expression. Quoting
where YAML does not require it is a convention of the bundle; omitting it where
YAML does require it is a parse error.

<a id="schema-10"></a>

### SCHEMA-10. Two union shapes are shorthand, expanded to their canonical form while validating

A union of `T` and `list[T]` accepts either, and a bare `T` expands to a
one-element list. A union of a non-class type `V` and a class declaring exactly
one required field whose type is `V` accepts either, and a bare `V` expands to
that class with the field set to it. An implementation expands shorthand while
validating, after the written value matches its union member, and a validated
document contains only the canonical form. No other union is shorthand.

Evidence: `schema-shorthand`, `rust/crates/yamaa-core/tests/schema_references.rs::collection_normalization_prioritizes_canonical_lists_even_when_t_is_a_list`, `rust/crates/yamaa-core/tests/schema_references.rs::shorthand_checks_the_written_member_without_reinterpreting_generated_fields`.
Replaces: REQ-0264, REQ-0265, REQ-0266, REQ-0267, REQ-0268.

The list and the class are the canonical forms. When a bare `V` expands,
declared defaults are applied to the remaining fields of the class and other
optional fields remain absent. This form also applies when `V` is
registry-backed, as `expression` is in `derivation`. Expansion happens after
the written value has been validated against the union member it matched, so a
constraint on the written form is checked before the value is expanded.

A union matching neither shape, such as `literal_value`, selects a member and
expands nothing. A rule may say where a shorthand applies and what the expanded
value means, but the rule must not define a different expansion.

## Descriptors

<a id="schema-11"></a>

### SCHEMA-11. A descriptor uses only the closed descriptor keywords, each where it applies

Only these descriptor keywords are supported: `type`, `required`,
`description`, `pattern`, `min_length`, `size`, `values` and `default`. `type`
is required and contains one type expression or a union. Every other keyword
is allowed only where the table below admits it. A `default` is allowed only
when `required` is false or absent, and it must satisfy the same descriptor.
An invalid descriptor keyword or default fails.

Evidence: `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::findings_follow_keyword_type_required_description_constraint_order`, `rust/crates/yamaa-core/tests/schema_inheritance_dependency_diagnostics.rs::invalid_types_and_required_flags_do_not_become_boolean_integer_coercions`, `rust/crates/yamaa-core/tests/schema_references.rs::invalid_default_preparation_never_bypasses_normalization_admission`.
Replaces: REQ-0269, REQ-0270, REQ-0271, REQ-0272, REQ-0273, REQ-0274, REQ-0275, REQ-0276, REQ-0284.

| Keyword | Allowed | Meaning |
|---|---|---|
| `type` | every descriptor; required | one type expression or a union |
| `required` | a class field descriptor only | whether the field must be written; defaults to false |
| `description` | every descriptor; optional | a non-empty string documenting the field or value type; it has no effect on validation |
| `pattern` | `str` only | a regular expression; [Text operations](../operations/text.md) owns its syntax, the engine that reads it, and what satisfying it means |
| `min_length` | `str` only | a minimum length, counted in [Text values](../values/text.md) scalar values |
| `size` | `list` or `dict` only | an exact size |
| `values` | `str` only | permitted values, compared by [Text values](../values/text.md) equality |
| `default` | when `required` is false or absent | a value satisfying the same descriptor |

Runtime semantics belong to the owning rule, which a description references.

<a id="schema-12"></a>

### SCHEMA-12. A value satisfies its type and every constraint its descriptor declares

When several constraints are present, all must pass. Defaults belong in the
schema, not only in prose. A value that fails its type or a constraint fails
validation, retaining the rejected value's declaring path and complete reason.

Fails: `invalid_field_type`, `value_not_permitted`, `pattern_mismatch`, `minimum_length`, `invalid_size`, `model_contract_mismatch`.
Evidence: `negative-str-case-unknown`, `negative-str-pad-width`, `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::captured_structural_projection_preserves_complete_ordered_constraint_outcome_and_authority`.
Replaces: REQ-0277, REQ-0287.

A value of the wrong type fails with `invalid_field_type`, a value outside
`values` with `value_not_permitted`, a string that does not satisfy `pattern`
with `pattern_mismatch`, a string shorter than `min_length` with
`minimum_length`, and a list or mapping of the wrong `size` with
`invalid_size`. Final normalized model admission reports
`model_contract_mismatch` for these causes.

<a id="schema-13"></a>

### SCHEMA-13. A class value carries every required field and no undeclared field

A value of a class carries every field its class declares required and no
field the class does not declare. An undeclared or missing class field fails.

Fails: `unknown_field`, `missing_required_field`, `model_contract_mismatch`.
Evidence: `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::custom_versionless_class_preserves_ordered_required_type_and_unknown_findings`, `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::final_model_findings_retain_no_pass_document_and_move_original_source`, `rust/crates/yamaa-core/tests/schema_references.rs::version_precedes_field_findings_and_classes_keep_schema_then_input_order`.
Replaces: REQ-0285.

Schema findings report an undeclared field as `unknown_field` and a missing
required field as `missing_required_field`. Final normalized model admission
reports `model_contract_mismatch` for the same undeclared or missing-field
causes.

<a id="schema-14"></a>

### SCHEMA-14. A descriptor written in flow form and in block form is the same descriptor

A descriptor may be written as a YAML flow mapping or in block form. The two
parse to the same mapping, and nothing in the schema language distinguishes
them. The bundle writes a class field descriptor in flow form without a
`description`, and in block form with a `description`; this is a convention
of how the bundle is written, not a validation requirement.

Evidence: `rust/crates/yamaa-adapters/src/yaml_decode/tests.rs::compact_flow_pairs_keep_nested_mapping_entries`, `rust/crates/yamaa-adapters/tests/declaration_schema_capture.rs::shipped_closure_retains_every_authoritative_byte_and_admits_without_a_directory`.
Replaces: REQ-0278, REQ-0279.

```yaml
example_class:
    - id: {type: str, required: true}
    - source:
        type: variable
        required: true
        description: Current-row value the record is matched on.
```

A document that mixes the forms differently is still valid.
