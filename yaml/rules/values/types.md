---
id: values/types
title: Types and conversion
status: normative
---

# Types and conversion

## Purpose

Define column types, missing normalization, compatibility, and conversion.

## Scope and dependencies

This contract owns the column vocabulary, missing normalization, input
compatibility, and conversion matrix. [Numeric values](numbers.md),
[text values](text.md), and [temporal values](temporal.md) own their value
spaces and representations. The [lifecycle](../execution/lifecycle.md) owns
conversion timing, [handlers](../execution/handlers.md) own replacements,
and [ingestion](../storage/ingestion.md) owns source-field typing.


## Requirements

<a id="req-0001"></a>

**REQ-0001.** A schema descriptor's `type`, a column's declared `type`, and
a value's runtime type are distinct namespaces. In
`- type: {type: column_type, required: true}`, the outer `type` names a
specification field and the inner `type` is a schema descriptor keyword.
Schema types include `str`, `int`, `float`, `bool`, `"null"`, `list`, `dict`,
and named types. Of these built-ins, only `str`, `int`, and `float` are also
column types, with the same runtime value meanings. Runtime types are
carried by values rather than declared at each expression input.

<a id="req-0002"></a>

**REQ-0002.** A column must declare exactly one of `str`, `int`, `float`,
`date`, or `datetime`. Each additionally admits missing. `str` follows the
text contract, numeric types follow the numbers contract, and temporal
types follow [Temporal values](temporal.md). There is no Boolean column type; flags use a `str`
column with `allowed_values`. Extending the vocabulary requires a rule
change and must not be an implementation-specific option.

<a id="req-0003"></a>

**REQ-0003.** Temporal values and canonical text follow [Temporal values](temporal.md). Other text remains
`str` and uses scalar ordering under the text contract. For values of one
temporal type, its canonical fixed-width text has the same chronological
order as the represented fields. This does not widen accepted temporal forms.

<a id="req-0004"></a>

**REQ-0004.** Named operation inputs must have compatible runtime types.
An operation must not implicitly convert one named input to the type of
another. Column conversion applies only to the completed derivation result
at the point defined by [Execution lifecycle](../execution/lifecycle.md).

<a id="req-0005"></a>

**REQ-0005.** `int` and `float` are mutually comparable using numeric
promotion under [Numeric values](numbers.md). Every other runtime type is comparable only with
itself. Collected temporal precision does not change the runtime type or
comparability. This compatibility rule applies to extremes, lookup key
pairs, ordering terms, and aggregate `MIN` and `MAX`; the type's owner
defines its order. Incompatible inputs fail under [REQ-0323](types.md#req-0323).

### Normalize values at their boundaries

<a id="req-0006"></a>

**REQ-0006.** Positive infinity, negative infinity, and every binary64 NaN
must become missing immediately at each of these boundaries:

- After YAML core-schema scalar resolution and before validating or using
  literals or defaults in specifications, schemas, environments, or
  conformance documents.
- After receiving a typed source value or parsing stored text as a number.
- After a built-in expression, mapping, conditional, coalescing operation,
  or handler selects or substitutes a result.
- After each numeric operator, scalar numeric function, or aggregate reduction.
- After a project binding returns a scalar and before checking its result contract.

Normalization must precede dispatch, missing handling, conversion, comparison,
equality, grouping, ordering, range selection, key validation, verification,
contract fingerprinting, and artifact rendering. Each consumer observes
missing and applies its own missing behavior; none may observe a non-finite
float. Artifacts and canonical values must have no infinity or NaN spelling.

<a id="req-0007"></a>

**REQ-0007.** Normalization is based on values, not text sentinels. An
unquoted YAML non-finite scalar resolves to a float and becomes missing.
The same characters quoted as a string remain text, including when stored
in a `str` destination. Only numeric parsing gives such text numeric meaning.

<a id="req-0008"></a>

**REQ-0008.** Normalization must not bypass a constraint prohibiting missing.
Non-missing keys and `not_missing` still reject the value. A function binding
returning a non-finite float satisfies its result contract only if that
contract allows missing through `may_return_missing: true`.

### Convert a completed result

<a id="req-0009"></a>

**REQ-0009.** At [Execution lifecycle](../execution/lifecycle.md)'s conversion stage, implementations must apply the
matrix below deterministically. A missing result stays missing without
attempting conversion and must not trigger `conversion_failure`.

<a id="req-0010"></a>

**REQ-0010.** The source is the value's runtime type; the destination is the
column's declared type. Identity preserves the value. `fail` is a conversion
failure. A Boolean fails conversion to every column type even when accepted
as a literal by the schema.

| Source | `str` | `int` | `float` | `date` | `datetime` |
| --- | --- | --- | --- | --- | --- |
| missing | missing | missing | missing | missing | missing |
| `str` | identity | numeric parse, then integral conversion | numeric parse | [Temporal values](temporal.md) parse | [Temporal values](temporal.md) parse |
| `int` | decimal text | identity | widening | fail | fail |
| `float` | shortest positional text | integral conversion | identity | fail | fail |
| `date` | [Temporal values](temporal.md) canonical text | fail | fail | identity | fail |
| `datetime` | [Temporal values](temporal.md) canonical text | fail | fail | fail | identity |
| `bool` | fail | fail | fail | fail | fail |

Numeric cells apply the numbers contract. Ingestion under [Source ingestion](../storage/ingestion.md) uses the same
`str` conversion row for a field's declared type.

<a id="req-0011"></a>

**REQ-0011.** Temporal cells apply [Temporal values](temporal.md)'s lexical forms, canonical text, and
prohibited conversions. Converting a temporal value to `str` retains the
canonical fields but does not retain collected precision.

### Interface behavior

<a id="req-1149"></a>

**REQ-1149.** The `literal_value` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `literal_value` | Scalar literal after [Types and conversion](types.md) normalizes any non-finite float to missing. |

<a id="req-1150"></a>

**REQ-1150.** The `module` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | Types shared by specification and project-environment schema entry points. |

## Error conditions

<a id="req-0012"></a>

**REQ-0012.** A declared column type outside the closed vocabulary must fail
schema validation under [Schema language](../reference/schema-language.md)'s `values` constraint.

<a id="req-0013"></a>

**REQ-0013.** A `fail` cell, unsuccessful parse, or undefined conversion must
raise conversion failure. Apply the [Local handlers](../execution/handlers.md) `conversion_failure` handler when
declared; otherwise the failure is fatal under [Execution lifecycle](../execution/lifecycle.md). Implementations must
not select an unspecified representation or silently substitute missing.
Numeric range and integrality failures are defined by [REQ-0021](numbers.md#req-0021).

### Errors

<a id="req-0323"></a>

**REQ-0323.** An input with an incompatible runtime type: fail.

<a id="req-0324"></a>

**REQ-0324.** A `sources` list or an ordering term mixing two runtime types
that are not mutually comparable: fail rather than convert an operand.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adsl-non-finite-values](../../../benchmark/adam-adsl-non-finite-values/README.md).
- [negative-column-type-unknown](../../../benchmark/negative-column-type-unknown/README.md).
- [negative-conversion-unparseable-number](../../../benchmark/negative-conversion-unparseable-number/README.md).
- [negative-greatest-incomparable-sources](../../../benchmark/negative-greatest-incomparable-sources/README.md).
- [negative-least-incomparable-sources](../../../benchmark/negative-least-incomparable-sources/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Define column types, missing normalization, compatibility, and conversion. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
