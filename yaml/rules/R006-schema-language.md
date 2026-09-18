---
id: R006
title: Compact Schema Language
status: normative
applies_to: [schema, root.schema_version, environment.schema_version]

---

# Compact schema language

## Intent

Define the modular notation in `schema.yaml` and `schema_*.yaml` for
consistent validation by R and Python implementations.

## Boundaries

This rule defines schema notation and structural validation: declarations,
descriptors, type expressions, and registries. R006 does not define a
declared field's run-time meaning. R007 populates the `expressions` registry
and gives it semantics. R011 defines the `column_type` vocabulary a
specification may declare.

## Schema bundle

**R006-1.** `schema.yaml` is the specification-bundle entry point, and
`schema_environment.yaml` is R018's independent project-environment-bundle
entry point. Every schema document is a YAML 1.2 mapping containing
`version`, an optional `includes`, and declarations. A document may be
included by both entry points; declaration uniqueness applies within each
loaded bundle.

**R006-2.** `includes` is an ordered list of filenames resolved relative to
the including file. Included filenames must match `schema_[a-z0-9_]+.yaml`.
Absolute paths, parent traversal, URLs, missing files, and include cycles
are errors. Every document in a bundle must declare the same version.
Include order has no validation or execution meaning.

**R006-3.** Implementations load the complete transitive bundle before
resolving names. Except for registries, a declaration name may occur once
in the bundle.

**R006-4.** A specification or project environment declares its entry-point
bundle version in `schema_version`. The declared version must equal the bundle
version. Any other version is an error, and the document is not validated
against the bundle.

**R006-5.** All schema documents must reject duplicate YAML keys, aliases,
merge keys, explicit tags, and unknown schema constructs.

**R006-6.** Schema documents and the documents they validate use R019's
ASCII source boundary. A decoded string value follows R019 even when written
with ASCII escape notation.

## Scalar resolution

**R006-7.** The schema bundle and every specification use the YAML 1.2 core
schema. Only `true`, `True`, `TRUE`, `false`, `False`, and `FALSE` resolve
to Boolean. Every other alphabetic scalar, including `y`, `Y`, `n`, `N`,
`yes`, `no`, `on`, and `off`, resolves to a string. The core schema has no
timestamp resolver, so an unquoted ISO-looking date or datetime is also a
string.

**R006-8.** Default parser settings do not satisfy this requirement. Each
implementation chooses how to meet the requirement, without requiring
authors to quote values. R011's non-finite normalization applies
core-schema scalar resolution.

## Named types

**R006-9.** A named type is a class, value descriptor, or registry-backed
type.

**R006-10.** A class is an ordered list of one-entry mappings. Each entry
maps an allowed field name to a descriptor:

```yaml
example_class:
    - name:
        type: str
        required: true
        description: Name of the example.
    - values: {type: "list[str]", required: false}
```

**R006-11.** Class fields are closed. Field order is descriptive and has no
execution meaning. Duplicate class field names are errors.

**R006-12.** A field name is not a descriptor keyword and may match one. In
`- type: {type: column_type, required: true}` the outer name is the field
`type` of `column_class` and the inner `type` is this rule's descriptor
keyword. The two uses are unrelated. R011 separates the vocabularies.

**R006-13.** A value type is a descriptor written directly as a mapping:

```yaml
identifier:
    type: str
    pattern: '^[A-Za-z_][A-Za-z0-9_]*$'
```

**R006-14.** A registry-backed type points to a schema registry:

```yaml
expression:
    registry: expressions
```

Its declaration contains exactly the `registry` keyword and cannot also
declare `type` or descriptor constraints.

**R006-15.** Named-type references may cross module boundaries. Recursive
types are allowed; a specification value is finite and must eventually match
a non-recursive type.

## Registries

**R006-16.** A registry is a mapping from a permitted keyword to its payload
shape. A registry is identified because a named type references it with
`registry`. Multiple modules may contribute entries to the same registry:

```yaml
expressions:
    mapping:
        - source: {type: variable, required: true}
        - dict: {type: "dict[str, literal_value]", required: true}
```

**R006-17.** Registry entry names must be unique across the complete bundle.
An entry's payload shape is either a class or a value descriptor.

**R006-18.** A value matching a registry-backed type must be a one-entry
mapping. Its key must exist in the referenced registry. Its value must match
that registry entry's payload shape. Registry declaration order has no
meaning.

**R006-19.** An unreferenced registry, an empty registry, an unknown
registry reference, or a duplicate registry entry is an error.

## Type expressions

**R006-20.** Built-in types are `str`, `int`, `float`, `bool`, `"null"`,
`list`, and `dict`. Boolean values are not integers. An `int` is accepted
where `float` is expected, but a `float` is not accepted where `int` is
expected.

```text
type_expression := type_name
                 | "list[" type_expression "]"
                 | "dict[" type_expression "," type_expression "]"
```

**R006-21.** Whitespace around nested expressions and the comma is ignored.
A YAML sequence of type expressions is a union. The quoted string `"null"`
is a type name. An unquoted YAML null is a value.

**R006-22.** `[` and `,` are structural characters inside a YAML flow
mapping or flow sequence, so a type expression containing either must be
quoted wherever it is written inside one. Block form imposes no such
requirement, but the bundle quotes a bracketed type expression in both:

```yaml
- keys: {type: "list[column_name]", required: true}
- parents: {type: [path, "list[path]"], required: false}
- order_by:
    type: "list[order_by_term]"
    required: false
    description: Terms ordering eligible records; declared with keep.
```

The quotes are YAML syntax and are not part of the type expression. Quoting
where YAML does not require it is a convention of the bundle; omitting it
where YAML does require it is a parse error.

## Shorthand unions

**R006-23.** Two union shapes are shorthand for a canonical form. An
implementation expands shorthand while validating. A validated document
contains only the canonical form. Both implementations agree on what they
validated.

**R006-24.** A union of `T` and `list[T]` accepts either. A bare `T`
expands to a one-element list, and the list is canonical.

**R006-25.** A union of a non-class type `V` and a class declaring exactly
one required field whose type is `V` accepts either. A bare `V` expands to
that class with the field set to it, and the class is canonical. Declared
defaults are applied to the remaining fields; other optional fields remain
absent. This form also applies when `V` is registry-backed, as `expression`
is in `derivation`.

**R006-26.** Expansion happens after the written value has been validated
against the union member it matched, so a constraint on the written form is
checked before the value is expanded.

**R006-27.** No other union is shorthand. A union matching neither shape,
such as `literal_value`, selects a member and expands nothing. A rule may
say where a shorthand applies and what the expanded value means, but the
rule must not define a different expansion.

## Descriptor keywords

Only these descriptor keywords are supported:

- **R006-28.** `type` is required and contains one type expression or a
  union.
- **R006-29.** `required` is allowed only in a class field descriptor. It
  defaults to false.
- **R006-30.** `description` is an optional non-empty string. It documents
  the declared field or value type and has no effect on validation. R007
  makes registry descriptions part of the operation-local language
  definition.
- **R006-31.** `pattern` is allowed only for `str` and carries a regular
  expression. R022 owns its syntax, the engine that reads it, and what
  satisfying it means.
- **R006-32.** `min_length` is allowed only for `str` and counts R019 scalar
  values.
- **R006-33.** `size` is allowed only for `list` or `dict` and requires an
  exact size.
- **R006-34.** `values` is allowed only for `str` and lists permitted values
  compared by R019 equality.
- **R006-35.** `default` is allowed only when `required` is false or absent
  and must satisfy the same descriptor.

**R006-36.** When several constraints are present, all must pass. Defaults
belong in the schema, not only in prose.

## Descriptor style

**R006-37.** A descriptor may be written as a YAML flow mapping or in block
form. The two parse to the same mapping. Nothing in this rule
distinguishes them.

```yaml
example_class:
    - id: {type: str, required: true}
    - source:
        type: variable
        required: true
        description: Current-row value the record is matched on.
```

**R006-38.** The bundle writes a class field descriptor in flow form without
a `description`, and in block form with a `description`. This is a convention
of how the bundle is written, not a validation requirement. A document that
mixes the forms differently is still valid.

## Rationale

A document written for one bundle version must not be silently reinterpreted
under another version. A version mismatch is rejected before validation, not
coerced. Closed classes and keyword sets make both implementations validate the
same documents. Undeclared content fails. One implementation cannot ignore
content that another honors. Shorthand unions expand to a canonical form. Both
implementations validate the canonical form. Written-form constraints are
checked before expansion. Uniform quoting of bracketed type expressions makes
types read the same in flow and block form. Flow descriptors without
descriptions keep classes readable as tables.

## Errors

Implementations must fail for:

- **R006-39.** invalid YAML or a prohibited YAML feature;
- **R006-40.** an invalid include, an inconsistent bundle version, or a
  specification whose `schema_version` does not equal the bundle version;
- **R006-41.** an unknown or duplicate type, registry, registry entry, or
  schema construct;
- **R006-42.** an invalid type expression or unresolved reference;
- **R006-43.** an invalid descriptor keyword or default;
- **R006-44.** an undeclared or missing class field;
- **R006-45.** a registry-backed value with zero, multiple, or unknown
  keywords;
- **R006-46.** a value that fails its type or constraints.
