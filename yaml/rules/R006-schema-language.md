---
id: R006
title: Compact Schema Language
status: normative
applies_to: [schema]
---

# Compact schema language

## Intent

Define the notation used by `schema.yaml` so R and Python implementations
validate specifications consistently.

## YAML document

`schema.yaml` uses YAML 1.2. Implementations must reject duplicate mapping
keys, aliases, merge keys, explicit tags, and unknown schema keywords.

The root is a mapping. `version` identifies this schema-language version. Every
other root key declares a named type. Named types must be unique and references
to unknown named types are errors.

The quoted string `"null"` is a type name. An unquoted YAML `null` is a null
value and must not be interpreted as a type name.

## Type declarations

A named type is either a class or a value type.

A class is an ordered list of one-entry mappings. Each entry maps a permitted
field name to a field descriptor:

```yaml
example_class:
    - name: {type: str, required: true}
    - values: {type: "list[str]", required: false}
```

Class fields are closed: a specification value must not contain a field that is
not declared by its class. Field order in the schema is descriptive and does
not prescribe execution order. Duplicate class field names are errors.

A value type is a descriptor written directly as a mapping:

```yaml
identifier:
    type: str
    pattern: '^[A-Za-z_][A-Za-z0-9_]*$'
```

Recursive named types are allowed. A specification value is finite and must
eventually match a non-recursive type.

## Type expressions

The built-in types are `str`, `int`, `float`, `bool`, `"null"`, `list`, and
`dict`. Boolean values are not integers. An `int` is accepted where `float` is
expected, but a `float` is not accepted where `int` is expected.

A type expression has this grammar:

```text
type_expression := type_name
                 | "list[" type_expression "]"
                 | "dict[" type_expression "," type_expression "]"
```

Whitespace surrounding a nested type expression or the comma is ignored. A
YAML sequence of type expressions is a union; a value is valid when it matches
at least one member.

```yaml
type: [path, "list[path]"]
```

## Descriptor keywords

Only these descriptor keywords are supported:

- `type` is required and contains one type expression or a union of them.
- `required` is permitted only in a class field descriptor. It is Boolean and
  defaults to `false` when omitted.
- `pattern` is permitted only for `str`. The value must match an ECMAScript
  regular expression. Matching searches the string unless the expression uses
  anchors such as `^` and `$`.
- `min_length` is permitted only for `str`. It is a non-negative integer and
  counts Unicode code points without trimming the value.
- `size` is permitted only for `list` or `dict`. It is a non-negative integer
  requiring exactly that many list items or mapping entries.

When more than one constraint is present, all constraints must pass. Constraint
keywords apply after a value matches `type`.

## Validation errors

Implementations must fail for:

- invalid YAML or a prohibited YAML feature;
- an unknown schema keyword or named type;
- an invalid type expression;
- a descriptor keyword used with an incompatible type;
- an undeclared class field;
- a missing required field;
- a value that does not match its type or every applicable constraint.
