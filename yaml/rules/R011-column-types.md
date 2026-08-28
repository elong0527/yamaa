---
id: R011
title: Column Type Vocabulary and Conversion
status: normative
applies_to: [column.type, column_type, derivation, conversion_failure]
depends_on: [R005, R006, R007, R010]
---

# Column type vocabulary and conversion

## Intent

Separate the three unrelated things this design calls a type, close the
vocabulary a column may declare, and define value conversion into a declared
type. R005 owns when conversion happens in the derivation lifecycle. This rule
owns what a declared type is and which conversions are defined.

## Three type namespaces

The word `type` appears in three roles. They are distinguished by position, not
by name, and no value is shared between their vocabularies except by
coincidence of spelling.

| Role | Where it is written | Vocabulary |
|---|---|---|
| Schema descriptor keyword | `type` inside a descriptor, in a class field or a value type | R006 type expressions over `str`, `int`, `float`, `bool`, `"null"`, `list`, `dict`, and named types |
| Declared column type | The `type` field of a `column_class` entry in a specification | `column_type` |
| Runtime value type | Never written; the type a value carries during evaluation | R007 type behavior |

`column_class` declares a field whose own name is `type`, so its declaration
reads `- type: {type: column_type, required: true}`. The outer `type` is a
specification field name and the inner `type` is the R006 descriptor keyword.
R006 resolves the two without ambiguity, because a class is an ordered list of
one-entry mappings whose keys are field names while a descriptor is the mapping
that follows. A class field name may coincide with a descriptor keyword and
carries no descriptor meaning when it does.

The schema vocabulary and the column vocabulary are not the same set. `str`,
`int`, and `float` are spelled the same in both and mean the same runtime
values. `date` is a column type and not a schema type. `bool`, `"null"`,
`list`, and `dict` are schema types and not column types.

## Closed column vocabulary

`column_type` is closed. A column declares exactly one of:

| Type | Values |
|---|---|
| `str` | A sequence of Unicode code points |
| `int` | A 64-bit signed integer, as defined by R010 |
| `float` | An IEEE 754 binary64 value, as defined by R010 |
| `date` | A complete proleptic Gregorian calendar date |

Every type additionally admits the missing value.

A `date` is a complete date. There is no month or year precision, so a partial
collected date is recovered, imputed, and reassembled as text before a `date`
column converts it. There is no `datetime` type: a value carrying a time of day
is declared `str`, and ISO 8601 text orders chronologically under R007
comparison. There is no Boolean column type; a flag is a `str` column with an
`allowed_values` verification, as the fixtures write it.

Extending this vocabulary is a rule change, not an implementation choice.

## Conversion

Conversion applies the completed derivation result to the declared column type
at the point R005 defines. Conversion is deterministic, and a conversion that
is not defined below fails rather than producing a substitute value.

A table row is the runtime type of the value being converted and a table
column is the declared type:

| From | to `str` | to `int` | to `float` | to `date` |
|---|---|---|---|---|
| missing | missing | missing | missing | missing |
| `str` | identity | parse, then numeric to `int` | parse | parse ISO 8601 |
| `int` | decimal text | identity | widen | fail |
| `float` | unresolved | integral only | identity | fail |
| `date` | ISO 8601 text | fail | fail | identity |
| `bool` | fail | fail | fail | fail |

A missing value converts to missing in every type. Conversion is not attempted,
so `conversion_failure` does not fire for a missing input and a missing result
is not a failure.

Parsing a `str` to a number accepts exactly R010's `number` production with an
optional leading `+` or `-` and no surrounding whitespace. Any other text
fails.

Parsing a `str` to a `date` accepts exactly a complete ISO 8601 calendar date
written `YYYY-MM-DD`. A partial date, a date with a time component, and a
non-date string all fail.

Converting a numeric value to `int` succeeds only when the value is exactly
integral and within the 64-bit signed range. A non-integral value fails. It is
neither truncated nor rounded: R010 states that a derivation must not round at
all and omits `ROUND` from its grammar for that reason, so a conversion that
quietly discarded a fractional part would reintroduce in the type system
exactly what the expression language excludes. Write `FLOOR`, `CEIL`, or
`TRUNC` in the `compute` expression to choose the integer explicitly, as R010
directs.

Widening an `int` to `float` is exact for magnitudes below 2^53 and is
otherwise the nearest binary64 value.

A `bool` never converts. No column type is Boolean, but `literal_value` admits
`true` and `false`, so a Boolean value can reach conversion. Failing is the
conservative reading: it is deterministic, no fixture depends on any other
outcome, and a later rule may define a mapping without invalidating a
specification written under this one.

## Unresolved

`float` to `str` is not defined here and remains open under finding 17 of
`examples/README.md`. It is the same decision as the decimal text an artifact
writes for a `float` column, which R005 also leaves open and which three
committed fixtures currently disagree about. A specification must not depend on
either until that decision is recorded. This rule does not close it, because
resolving it changes committed expected outputs.

Source-format value recognition remains open under finding 15. This rule
governs conversion of a value that evaluation already produced; it does not say
how a reader decides that a source field is missing rather than empty text.

## Errors

- A `column.type` outside `column_type`: schema failure under R006 `values`.
- A conversion listed as `fail`, or a parse that does not match: conversion
  failure, handled by `conversion_failure` under R008 and otherwise fatal
  under R005.
- A numeric value outside the 64-bit signed range converted to `int`: fail.
- A non-integral numeric value converted to `int`: fail.
- Reliance on an unresolved conversion: fail rather than choose a
  representation.
