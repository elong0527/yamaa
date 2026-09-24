---
id: reference/schema-language
title: Schema language
status: normative
---

# Schema language

## Purpose

Defines schema notation, registries, constraints, and canonical shorthand.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Project functions](../operations/functions.md).
- [Text operations](../operations/text.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).

## Requirements

### Schema bundle

<a id="req-0242"></a>

**REQ-0242.** `schema.yaml` is the specification-bundle entry point, and
`schema_environment.yaml` is [Project functions](../operations/functions.md)'s independent project-environment-bundle
entry point. Every schema document is a YAML 1.2 mapping containing
`version`, an optional `includes`, and declarations. A document may be
included by both entry points; declaration uniqueness applies within each
loaded bundle.

<a id="req-0243"></a>

**REQ-0243.** `includes` is an ordered list of filenames resolved relative to
the including file. Included filenames must match `schema_[a-z0-9_]+.yaml`.
Absolute paths, parent traversal, URLs, missing files, and include cycles
are errors. Every document in a bundle must declare the same version.
Include order has no validation or execution meaning.

<a id="req-0244"></a>

**REQ-0244.** Implementations load the complete transitive bundle before
resolving names. Except for registries, a declaration name may occur once
in the bundle.

<a id="req-0245"></a>

**REQ-0245.** A specification or project environment declares its entry-point
bundle version in `schema_version`. The declared version must equal the bundle
version. Any other version is an error, and the document is not validated
against the bundle.

<a id="req-0246"></a>

**REQ-0246.** All schema documents must reject duplicate YAML keys, aliases,
merge keys, explicit tags, and unknown schema constructs.

<a id="req-0247"></a>

**REQ-0247.** Schema documents and the documents they validate use [Text values](../values/text.md)'s
ASCII source boundary. A decoded string value follows [Text values](../values/text.md) even when written
with ASCII escape notation.

### Scalar resolution

<a id="req-0248"></a>

**REQ-0248.** The schema bundle and every specification use the YAML 1.2 core
schema. Only `true`, `True`, `TRUE`, `false`, `False`, and `FALSE` resolve
to Boolean. Every other alphabetic scalar, including `y`, `Y`, `n`, `N`,
`yes`, `no`, `on`, and `off`, resolves to a string. The core schema has no
timestamp resolver, so an unquoted ISO-looking date or datetime is also a
string.

<a id="req-0249"></a>

**REQ-0249.** Default parser settings do not satisfy this requirement.
Implementations must meet it without requiring authors to quote values.
[Types and conversion](../values/types.md)'s non-finite normalization
applies immediately after core-schema scalar resolution.

### Named types

<a id="req-0250"></a>

**REQ-0250.** A named type is a class, value descriptor, or registry-backed
type.

<a id="req-0251"></a>

**REQ-0251.** A class is an ordered list of one-entry mappings. Each entry
maps an allowed field name to a descriptor:

```yaml
example_class:
    - name:
        type: str
        required: true
        description: Name of the example.
    - values: {type: "list[str]", required: false}
```

<a id="req-0252"></a>

**REQ-0252.** Class fields are closed. Field order is descriptive and has no
execution meaning. Duplicate class field names are errors.

<a id="req-0253"></a>

**REQ-0253.** A field name is not a descriptor keyword and may match one. In
`- type: {type: column_type, required: true}` the outer name is the field
`type` of `column_class` and the inner `type` is this contract's descriptor
keyword. The two uses are unrelated. [Types and conversion](../values/types.md) separates the vocabularies.

<a id="req-0254"></a>

**REQ-0254.** A value type is a descriptor written directly as a mapping:

```yaml
identifier:
    type: str
    pattern: '^[A-Za-z_][A-Za-z0-9_]*$'
```

<a id="req-0255"></a>

**REQ-0255.** A registry-backed type points to a schema registry:

```yaml
expression:
    registry: expressions
```

Its declaration contains exactly the `registry` keyword and cannot also
declare `type` or descriptor constraints.

<a id="req-0256"></a>

**REQ-0256.** Named-type references may cross module boundaries. Recursive
types are allowed; a specification value is finite and must eventually match
a non-recursive type.

### Registries

<a id="req-0257"></a>

**REQ-0257.** A registry is a mapping from a permitted keyword to its payload
shape. A registry is identified by a named type referencing it with
`registry`. Multiple modules may contribute entries to the same registry:

```yaml
expressions:
    mapping:
        - source: {type: variable, required: true}
        - dict: {type: "dict[str, literal_value]", required: true}
```

<a id="req-0258"></a>

**REQ-0258.** Registry entry names must be unique across the complete bundle.
An entry's payload shape is either a class or a value descriptor.

<a id="req-0259"></a>

**REQ-0259.** A value matching a registry-backed type must be a one-entry
mapping. Its key must exist in the referenced registry. Its value must match
that registry entry's payload shape. Registry declaration order has no
meaning.

<a id="req-0260"></a>

**REQ-0260.** An unreferenced registry, an empty registry, an unknown
registry reference, or a duplicate registry entry is an error.

### Type expressions

<a id="req-0261"></a>

**REQ-0261.** Built-in types are `str`, `int`, `float`, `bool`, `"null"`,
`list`, and `dict`. Boolean values are not integers. An `int` is accepted
where `float` is expected, but a `float` is not accepted where `int` is
expected.

```text
type_expression := type_name
                 | "list[" type_expression "]"
                 | "dict[" type_expression "," type_expression "]"
```

<a id="req-0262"></a>

**REQ-0262.** Whitespace around nested expressions and the comma is ignored.
A YAML sequence of type expressions is a union. The quoted string `"null"`
is a type name. An unquoted YAML null is a value.

<a id="req-0263"></a>

**REQ-0263.** `[` and `,` are structural characters inside a YAML flow
mapping or flow sequence, so a type expression containing either must be
quoted wherever it is written inside one. Block form imposes no such
requirement, but the bundle quotes a bracketed type expression in both:

```yaml
- keys: {type: "list[identifier]", required: true}
- parents: {type: [path, "list[path]"], required: false}
- order_by:
    type: "list[order_by_term]"
    required: false
    description: Terms ordering eligible records; declared with keep.
```

The quotes are YAML syntax and are not part of the type expression. Quoting
where YAML does not require it is a convention of the bundle; omitting it
where YAML does require it is a parse error.

### Shorthand unions

<a id="req-0264"></a>

**REQ-0264.** Two union shapes are shorthand for a canonical form. An
implementation expands shorthand while validating. A validated document
contains only the canonical form. Both implementations agree on what they
validated.

<a id="req-0265"></a>

**REQ-0265.** A union of `T` and `list[T]` accepts either. A bare `T`
expands to a one-element list, and the list is canonical.

<a id="req-0266"></a>

**REQ-0266.** A union of a non-class type `V` and a class declaring exactly
one required field whose type is `V` accepts either. A bare `V` expands to
that class with the field set to it, and the class is canonical. Declared
defaults are applied to the remaining fields; other optional fields remain
absent. This form also applies when `V` is registry-backed, as `expression`
is in `derivation`.

<a id="req-0267"></a>

**REQ-0267.** Expansion happens after the written value has been validated
against the union member it matched, so a constraint on the written form is
checked before the value is expanded.

<a id="req-0268"></a>

**REQ-0268.** No other union is shorthand. A union matching neither shape,
such as `literal_value`, selects a member and expands nothing. A rule may
say where a shorthand applies and what the expanded value means, but the
rule must not define a different expansion.

### Descriptor keywords

<a id="req-0269"></a>

**REQ-0269.** Only these descriptor keywords are supported:

`type` is required and contains one type expression or a
  union.

<a id="req-0270"></a>

**REQ-0270.** `required` is allowed only in a class field descriptor. It
  defaults to false.

<a id="req-0271"></a>

**REQ-0271.** `description` is an optional non-empty string documenting the field or
value type. It has no effect on validation. Runtime semantics belong to the
owning contract, which the description references.

<a id="req-0272"></a>

**REQ-0272.** `pattern` is allowed only for `str` and carries a regular
  expression. [Text operations](../operations/text.md) owns its syntax, the engine that reads it, and what
  satisfying it means.

<a id="req-0273"></a>

**REQ-0273.** `min_length` is allowed only for `str` and counts [Text values](../values/text.md) scalar
  values.

<a id="req-0274"></a>

**REQ-0274.** `size` is allowed only for `list` or `dict` and requires an
  exact size.

<a id="req-0275"></a>

**REQ-0275.** `values` is allowed only for `str` and lists permitted values
  compared by [Text values](../values/text.md) equality.

<a id="req-0276"></a>

**REQ-0276.** `default` is allowed only when `required` is false or absent
  and must satisfy the same descriptor.

<a id="req-0277"></a>

**REQ-0277.** When several constraints are present, all must pass. Defaults
belong in the schema, not only in prose.

### Descriptor style

<a id="req-0278"></a>

**REQ-0278.** A descriptor may be written as a YAML flow mapping or in block
form. The two parse to the same mapping. Nothing in this contract
distinguishes them.

```yaml
example_class:
    - id: {type: str, required: true}
    - source:
        type: variable
        required: true
        description: Current-row value the record is matched on.
```

<a id="req-0279"></a>

**REQ-0279.** The bundle writes a class field descriptor in flow form without
a `description`, and in block form with a `description`. This is a convention
of how the bundle is written, not a validation requirement. A document that
mixes the forms differently is still valid.

## Error conditions

<a id="req-0280"></a>

**REQ-0280.** Implementations must fail for:

invalid YAML or a prohibited YAML feature;

<a id="req-0281"></a>

**REQ-0281.** an invalid include, an inconsistent bundle version, or a
  specification whose `schema_version` does not equal the bundle version;

<a id="req-0282"></a>

**REQ-0282.** an unknown or duplicate type, registry, registry entry, or
  schema construct;

<a id="req-0283"></a>

**REQ-0283.** an invalid type expression or unresolved reference;

<a id="req-0284"></a>

**REQ-0284.** an invalid descriptor keyword or default;

<a id="req-0285"></a>

**REQ-0285.** an undeclared or missing class field;

<a id="req-0286"></a>

**REQ-0286.** a registry-backed value with zero, multiple, or unknown
  keywords;

<a id="req-0287"></a>

**REQ-0287.** a value that fails its type or constraints.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-literal-structure](../../benchmarks/negative-literal-structure/README.md).
- [negative-rank-bad-method](../../benchmarks/negative-rank-bad-method/README.md).
- [negative-row-number-filter](../../benchmarks/negative-row-number-filter/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Keeping this topic in one contract lets other owners refer to it without
defining a second policy.
