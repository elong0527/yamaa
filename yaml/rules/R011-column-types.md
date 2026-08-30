---
id: R011
title: Column Type Vocabulary and Conversion
status: normative
applies_to: [column.type, column_type, derivation, conversion_failure]
depends_on: [R005, R006, R007, R008, R010, R016]
---

# Column type vocabulary and conversion

## Intent

Close the vocabulary a column may declare, and define value conversion into a
declared type.

## Boundaries

This rule owns what a declared type is and which conversions are defined. R005
owns when conversion happens in the derivation lifecycle and what an unhandled
failure does to the run. R008 owns `conversion_failure`. R010 owns the
arithmetic that produces a numeric value in the first place. R014 owns the
other end: which stored fields are missing and what type a bound value carries
before any conversion is reached. It applies this rule's `str` row to a field's
declared type, so text is parsed the same way wherever it is read. R016 owns
the datetime value: its lexical form, its zone and precision model, its
comparison, and its canonical text are stated there once and used by the cells
below.

## Three type namespaces

The word `type` appears in three roles, distinguished by position rather than
by name. No value is shared between their vocabularies except by coincidence of
spelling.

| Role | Where it is written | Vocabulary |
|---|---|---|
| Schema descriptor keyword | `type` inside a descriptor, in a class field or a value type | R006 type expressions over `str`, `int`, `float`, `bool`, `"null"`, `list`, `dict`, and named types |
| Declared column type | The `type` field of a `column_class` entry in a specification | `column_type`, below |
| Runtime value type | Never written; the type a value carries during evaluation | R007 type behavior |

`column_class` declares a field whose own name is `type`, so its declaration
reads `- type: {type: column_type, required: true}`. The outer `type` is a
specification field name and the inner `type` is the R006 descriptor keyword;
R006 resolves the two without ambiguity.

The schema vocabulary and the column vocabulary are not the same set. `str`,
`int`, and `float` are spelled the same in both and mean the same runtime
values. `date` and `datetime` are column types and not schema types. `bool`,
`"null"`, `list`, and `dict` are schema types and not column types.

## Closed column vocabulary

`column_type` is closed. A column declares exactly one of:

| Type | Values |
|---|---|
| `str` | A sequence of Unicode code points |
| `int` | A 64-bit signed integer, as defined by R010 |
| `float` | An IEEE 754 binary64 value, as defined by R010 |
| `date` | A complete proleptic Gregorian calendar date |
| `datetime` | A complete local civil datetime, as defined by R016 |

Every type additionally admits the missing value.

A `date` is a complete date. There is no month or year precision, so a partial
collected date is carried as text and completed before it becomes a `date`.
`date_impute` performs that completion as a declared rule rather than as string
surgery; its result is a `date` like any other, and nothing distinguishes it
from a fully collected one. `date_precision` reads how much of a date the
collected text carried, so a specification can record beside the date what it
supplied; the date value itself still carries no precision.

A `datetime` is complete in the same sense and on the same terms: it carries a
date and a time of day and is never partial, so a truncated collected value is
text until something completes it. It is a wall-clock reading rather than an
instant, because R016 admits no zone and no offset, and it resolves to a whole
second. A value that must keep the characters it was collected with, or that
carries a sub-second time, stays `str`, and its ISO 8601 text orders
chronologically under R007 comparison.

There is no Boolean column type; a flag is a `str` column with an
`allowed_values` verification, as the examples write it.

Extending this vocabulary is a rule change, not an implementation choice.

## Conversion

Conversion applies the completed derivation result to the declared column type
at the point R005 defines. Conversion is deterministic, and a conversion that
is not defined below fails rather than producing a substitute value.

A table row is the runtime type of the value being converted and a table
column is the declared type:

| From | to `str` | to `int` | to `float` | to `date` | to `datetime` |
|---|---|---|---|---|---|
| missing | missing | missing | missing | missing | missing |
| `str` | identity | parse, then numeric to `int` | parse | parse ISO 8601 | parse R016's form |
| `int` | decimal text | identity | widen | fail | fail |
| `float` | decimal text, see below | integral only | identity | fail | fail |
| `date` | ISO 8601 text | fail | fail | identity | fail |
| `datetime` | R016's canonical text | fail | fail | fail | identity |
| `bool` | fail | fail | fail | fail | fail |

A missing value converts to missing in every type. Conversion is not attempted,
so `conversion_failure` does not fire for a missing input and a missing result
is not a failure.

Parsing a `str` to a number accepts exactly R010's `number` production with an
optional leading `+` or `-` and no surrounding whitespace. Any other text
fails.

Parsing a `str` to a `date` accepts exactly a complete ISO 8601 calendar date
written `YYYY-MM-DD`. A partial date, a date with a time component, and a
non-date string all fail.

Parsing a `str` to a `datetime` accepts exactly the lexical form R016 fixes,
and a `datetime` becomes text as the canonical form R016 fixes. Both are stated
there rather than here, so the grammar a column conversion applies and the
grammar any later reader applies cannot drift apart.

A `date` and a `datetime` do not convert into each other, in either direction.
`date` to `datetime` would invent a time of day, and `datetime` to `date` would
discard a collected one; each would decide silently what a specification never
stated, which is the same reason a non-integral `float` does not become an
`int`. An operation that extracts a date or composes a datetime states that
intent explicitly, and R016 records that neither is registered yet.

Converting a numeric value to `int` succeeds only when the value is exactly
integral and within the 64-bit signed range. A non-integral value fails; it is
neither truncated nor rounded, because a conversion that quietly discarded a
fractional part would reintroduce in the type system exactly what R010 excludes
from the expression language. Write `FLOOR`, `CEIL`, or `TRUNC` in the
`compute` expression to choose the integer explicitly.

Widening an `int` to `float` is exact for magnitudes below 2^53 and is
otherwise the nearest binary64 value.

A `bool` never converts. No column type is Boolean, but `literal_value` admits
`true` and `false`, so a Boolean value can reach conversion. Failing is the
conservative reading: it is deterministic, no example depends on any other
outcome, and a later rule may define a mapping without invalidating a
specification written under this one.

## Float to text

A `float` becomes text in two places: this conversion, and the decimal text an
artifact writes for a `float` column. Both use the same form, so a `str` column
derived from a `float` and the artifact's rendering of that same `float` never
disagree, and rendering never changes a stored value.

**The schema fixes no precision.** How many decimal places a value carries is a
property of the study and its instruments, not of the derivation language. The
form is therefore a project setting, resolved from the global project
configuration in the same way `function` resolves its environment:

- With no project setting, a `float` renders as the shortest decimal text that
  parses back to the same binary64 value, with a trailing `.0` omitted for an
  integral value. This is lossless and is the default because it cannot silently
  discard a derived digit.
- A project may declare a number of decimal places. The value is then rendered
  with exactly that many places, trailing zeros removed, and a value with no
  fractional part written without a decimal point.

Rendering never changes a stored value. A `float` column keeps full binary64
precision for every comparison, verification, and dependent derivation; only
its text form is affected. A `str` column derived from a `float` does store the
rendered text, and from that point it is a string like any other.

The example suite declares **four decimal places**, which is what its committed
expected outputs record.

## Errors

- A `column.type` outside `column_type`: schema failure under R006 `values`.
- A conversion listed as `fail`, or a parse that does not match: conversion
  failure, handled by `conversion_failure` under R008 and otherwise fatal
  under R005.
- A numeric value outside the 64-bit signed range converted to `int`: fail.
- A non-integral numeric value converted to `int`: fail.
- Reliance on an unresolved conversion: fail rather than choose a
  representation.
