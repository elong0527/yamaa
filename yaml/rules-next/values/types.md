# Types, missing values, and conversion

Status: non-normative draft for issue #606.

## Purpose

Define the values a column can carry and how a completed derivation becomes
one of those values.

## Scope and dependencies

This contract owns the column vocabulary, missing normalization, input
compatibility, and conversion matrix. [Numbers](numbers.md) owns numeric
representation and conversion details; [text](text.md) owns string identity
and order. Pending its rewrite, [R016](../../rules/R016-temporal-values.md)
owns temporal values and their text forms.

[R005](../../rules/R005-output-contract.md) owns conversion timing and
unhandled failures; [R008](../../rules/R008-local-handlers.md) owns handlers.
[R014](../../rules/R014-source-ingestion.md) owns source-field typing and
structural missingness. [R006](../../rules/R006-schema-language.md) owns
schema notation, and [R018](../../rules/R018-project-function-environment.md)
owns function contracts. These dependencies remain authoritative during
the draft; they are not additional definitions here.

## Requirements

**REQ-0001.** A schema descriptor's `type`, a column's declared `type`, and
a value's runtime type are distinct namespaces. In
`- type: {type: column_type, required: true}`, the outer `type` names a
specification field and the inner `type` is a schema descriptor keyword.
Schema types include `str`, `int`, `float`, `bool`, `"null"`, `list`, `dict`,
and named types. Of these built-ins, only `str`, `int`, and `float` are also
column types, with the same runtime value meanings. Runtime types are
carried by values rather than declared at each expression input.

**REQ-0002.** A column must declare exactly one of `str`, `int`, `float`,
`date`, or `datetime`. Each additionally admits missing. `str` follows the
text contract, numeric types follow the numbers contract, and temporal
types follow R016. There is no Boolean column type; flags use a `str`
column with `allowed_values`. Extending the vocabulary requires a rule
change and must not be an implementation-specific option.

**REQ-0003.** Temporal value spaces, accepted text, canonical text, and
ordering follow R016. Text that neither temporal type admits remains
`str`; this contract does not invent another temporal type or conversion.
ISO 8601 text orders chronologically under R007 comparison, as stated by
the existing R011-6 contract; this does not widen R016's accepted forms.

**REQ-0004.** Named operation inputs must have compatible runtime types.
An operation must not implicitly convert one named input to the type of
another. Column conversion applies only to the completed derivation result
at the point defined by R005.

**REQ-0005.** `int` and `float` are mutually comparable using numeric
promotion under R010. Every other runtime type is comparable only with
itself. Collected temporal precision does not change the runtime type or
comparability. This compatibility rule applies to extremes, lookup key
pairs, ordering terms, and aggregate `MIN` and `MAX`; the type's owner
defines its order. Incompatible inputs fail under R007-38.

### Normalize values at their boundaries

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

**REQ-0007.** Normalization is based on values, not text sentinels. An
unquoted YAML non-finite scalar resolves to a float and becomes missing.
The same characters quoted as a string remain text, including when stored
in a `str` destination. Only numeric parsing gives such text numeric meaning.

**REQ-0008.** Normalization must not bypass a constraint prohibiting missing.
Non-missing keys and `not_missing` still reject the value. A function binding
returning a non-finite float satisfies its result contract only if that
contract allows missing through `may_return_missing: true`.

### Convert a completed result

**REQ-0009.** At R005's conversion stage, implementations must apply the
matrix below deterministically. A missing result stays missing without
attempting conversion and must not trigger `conversion_failure`.

**REQ-0010.** The source is the value's runtime type; the destination is the
column's declared type. Identity preserves the value. `fail` is a conversion
failure. A Boolean fails conversion to every column type even when accepted
as a literal by the schema.

| Source | `str` | `int` | `float` | `date` | `datetime` |
| --- | --- | --- | --- | --- | --- |
| missing | missing | missing | missing | missing | missing |
| `str` | identity | numeric parse, then integral conversion | numeric parse | R016 parse | R016 parse |
| `int` | decimal text | identity | widening | fail | fail |
| `float` | shortest positional text | integral conversion | identity | fail | fail |
| `date` | R016 canonical text | fail | fail | identity | fail |
| `datetime` | R016 canonical text | fail | fail | fail | identity |
| `bool` | fail | fail | fail | fail | fail |

Numeric cells apply the numbers contract. Ingestion under R014 uses the same
`str` conversion row for a field's declared type.

**REQ-0011.** Temporal cells apply R016's lexical forms, canonical text, and
prohibited conversions. Converting a temporal value to `str` retains the
canonical fields but does not retain collected precision.

## Error conditions

**REQ-0012.** A declared column type outside the closed vocabulary must fail
schema validation under R006's `values` constraint.

**REQ-0013.** A `fail` cell, unsuccessful parse, or undefined conversion must
raise conversion failure. Apply the R008 `conversion_failure` handler when
declared; otherwise the failure is fatal under R005. Implementations must
not select an unspecified representation or silently substitute missing.
Numeric range and integrality failures are defined by REQ-0021.

## Conformance examples

Existing specifications and expected results remain unchanged:

- [Non-finite values](../../../benchmark/adam-adsl-non-finite-values/README.md)
  exercises normalization and downstream missing behavior.
- [Fractional value converted to int](../../../benchmark/negative-conversion-non-integral/README.md)
  pins the failure at the conversion stage.
- [Unparseable numeric text](../../../benchmark/negative-conversion-unparseable-number/README.md)
  rejects text outside the numeric grammar.
- [Incomplete date conversion](../../../benchmark/negative-conversion-incomplete-date/README.md)
  preserves R016's temporal boundary.

These are representative fixtures, not a claim of complete runtime coverage
for every requirement. The migration map records the full legacy provenance.

## Rationale

A closed conversion matrix separates type compatibility from conversion.
Normalizing at boundaries gives every downstream operation the same missing
value and prevents host infinity and NaN behavior from entering the contract.
