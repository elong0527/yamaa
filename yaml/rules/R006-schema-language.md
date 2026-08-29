---
id: R006
title: Compact Schema Language
status: normative
applies_to: [schema]
---

# Compact schema language

## Intent

Define the modular notation used by `schema.yaml` and `schema_*.yaml` so R and
Python implementations validate specifications consistently.

## Schema bundle

`schema.yaml` is the bundle entry point. Every schema document is a YAML 1.2
mapping containing `version`, an optional `includes`, and declarations.

`includes` is an ordered list of filenames resolved relative to the including
file. Included filenames must match `schema_[a-z0-9_]+.yaml`; absolute paths,
parent traversal, URLs, missing files, and include cycles are errors. Every
document in a bundle must declare the same version. Include order has no
validation or execution meaning.

Implementations load the complete transitive bundle before resolving names.
Except for registries, a declaration name may occur only once in the bundle.

A specification declares the bundle it is written against in `schema_version`.
It must equal the bundle version. A specification declaring any other version
is an error and is not validated against this bundle, so a later version can be
introduced without silently reinterpreting a specification written for an
earlier one.

All schema documents must reject duplicate YAML keys, aliases, merge keys,
explicit tags, and unknown schema constructs.

## Scalar resolution

The schema bundle and every specification use the YAML 1.2 core schema. Only
`true` and `false` resolve to Boolean. Every other alphabetic scalar, including
`y`, `Y`, `n`, `N`, `yes`, `no`, `on`, and `off`, resolves to a string.

Default parser settings do not satisfy this requirement. How an implementation
meets it is its own choice, but it must not be met by requiring authors to
quote values.

## Named types

A named type is a class, value descriptor, or registry-backed type.

A class is an ordered list of one-entry mappings. Each entry maps an allowed
field name to a descriptor:

```yaml
example_class:
    - name: {type: str, required: true, description: Name of the example.}
    - values: {type: "list[str]", required: false}
```

Class fields are closed. Field order is descriptive and has no execution
meaning. Duplicate class field names are errors.

A field name is not a descriptor keyword and may coincide with one. In
`- type: {type: column_type, required: true}` the outer name is the field
`type` of `column_class` and the inner `type` is this rule's descriptor
keyword. The two are unrelated; R011 separates the vocabularies.

A value type is a descriptor written directly as a mapping:

```yaml
identifier:
    type: str
    pattern: '^[A-Za-z_][A-Za-z0-9_]*$'
```

A registry-backed type points to a schema registry:

```yaml
expression:
    registry: expressions
```

Its declaration contains exactly the `registry` keyword and cannot also declare
`type` or descriptor constraints.

Named-type references may cross module boundaries. Recursive types are allowed;
a specification value is finite and must eventually match a non-recursive type.

## Registries

A registry is a mapping from a permitted keyword to its payload shape. A
registry is identified because a named type references it with `registry`.
Multiple modules may contribute entries to the same registry:

```yaml
expressions:
    mapping:
        - source: {type: variable, required: true}
        - dict: {type: "dict[str, literal_value]", required: true}
```

Registry entry names must be unique across the complete bundle. An entry's
payload shape is either a class or a value descriptor.

A value matching a registry-backed type must be a mapping with exactly one
entry. Its key must exist in the referenced registry, and its value must match
that entry's payload shape. Registry declaration order has no meaning.

An unreferenced registry, an empty registry, an unknown registry reference, or
a duplicate registry entry is an error.

## Type expressions

Built-in types are `str`, `int`, `float`, `bool`, `"null"`, `list`, and `dict`.
Boolean values are not integers. An `int` is accepted where `float` is expected,
but a `float` is not accepted where `int` is expected.

```text
type_expression := type_name
                 | "list[" type_expression "]"
                 | "dict[" type_expression "," type_expression "]"
```

Whitespace around nested expressions and the comma is ignored. A YAML sequence
of type expressions is a union. The quoted string `"null"` is a type name; an
unquoted YAML null is a value.

## Shorthand unions

Two union shapes are shorthand for a canonical form. An implementation expands
a shorthand while validating, so a validated document contains only the
canonical form and two implementations agree on what they validated.

A union of `T` and `list[T]` accepts either. A bare `T` expands to a
one-element list, and the list is canonical.

A union of a value type `V` and a class declaring exactly one required field
whose type is `V` accepts either. A bare `V` expands to that class with the
field set to it, and the class is canonical. The remaining fields take their
declared defaults.

Expansion happens after the written value has been validated against the union
member it matched, so a constraint on the written form is checked before the
value is expanded.

No other union is shorthand. A union matching neither shape, such as
`literal_value`, selects a member and expands nothing. These are the only
shorthand mechanisms in the language. A rule may say where a shorthand applies
and what the expanded value means, but must not define a different expansion.

## Descriptor keywords

Only these descriptor keywords are supported:

- `type` is required and contains one type expression or a union.
- `required` is allowed only in a class field descriptor. It defaults to false.
- `description` is an optional non-empty string. It documents the declared
  field or value type and has no effect on validation. R007 makes registry
  descriptions part of the operation-local language definition.
- `pattern` is allowed only for `str` and is an ECMAScript regular expression.
- `min_length` is allowed only for `str` and counts Unicode code points.
- `size` is allowed only for `list` or `dict` and requires an exact size.
- `values` is allowed only for `str` and lists exact permitted values.
- `default` is allowed only when `required` is false or absent and must satisfy
  the same descriptor.

When several constraints are present, all must pass. Defaults belong in the
schema, not only in prose.

## Validation errors

Implementations must fail for:

- invalid YAML or a prohibited YAML feature;
- an invalid include, an inconsistent bundle version, or a specification
  whose `schema_version` does not equal the bundle version;
- an unknown or duplicate type, registry, registry entry, or schema construct;
- an invalid type expression or unresolved reference;
- an invalid descriptor keyword or default;
- an undeclared or missing class field;
- a registry-backed value with zero, multiple, or unknown keywords;
- a value that fails its type or constraints.
