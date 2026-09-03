---
id: R011
title: Column Type Vocabulary, Missing Normalization, and Conversion
status: normative
applies_to: [column.type, column_type, literal_value, derivation, conversion_failure]
depends_on: [R005, R006, R007, R008, R009, R010, R014, R016, R018]
---

# Column type vocabulary and conversion

## Intent

Close the vocabulary a column may declare, normalize non-finite floats, and
define value conversion into a declared type.

## Boundaries

This rule owns what a declared type is, normalization of any non-finite float
that enters or is produced by the language, and which conversions are defined.
R005 owns when conversion happens in the derivation lifecycle and what an
unhandled failure does to the run. R008 owns `conversion_failure`. R010 owns
the arithmetic that produces a numeric value in the first place. R014 owns the
other end: which stored fields are structurally missing and what type a bound
value carries before conversion or normalization is reached. It applies this
rule's `str` row to a field's declared type, so text is parsed the same way
wherever it is read. R016 owns both temporal types. What a `date` and a
`datetime` denote, the text each is read from and written back to, how two of
them order, and which operations read them are stated there; the temporal
cells below apply that rule rather than restating it. R018 owns the
function-only Boolean parameter type and the result contract applied after
this rule's normalization.

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
| `date` | A calendar date, as defined by R016 |
| `datetime` | A local civil datetime, as defined by R016 |

Every type additionally admits the missing value.

`date` and `datetime` are the two temporal types, and R016 defines both: what
each admits, the text it is read from and written back to, how two of them
order, and which operations read them. This rule adds nothing to that
definition. A value neither type admits is a `str` like any other, and ISO
8601 text orders chronologically under R007 comparison.

There is no Boolean column type; a flag is a `str` column with an
`allowed_values` verification, as the examples write it.

Extending this vocabulary is a rule change, not an implementation choice.

## Non-finite floats are missing

A non-finite float is positive infinity, negative infinity, or any NaN binary64
value. Every non-finite float is the missing value. It is normalized
immediately at every boundary where a float enters the language or a numeric
operation produces one:

- after YAML 1.2 core-schema scalar resolution in a specification, schema,
  project environment, or conformance document, before a literal or default is
  validated or used;
- after a self-describing source supplies a typed value or stored text is
  parsed as a number;
- after a built-in expression, mapping, conditional, coalescing operation, or
  handler selects or substitutes a result;
- after each numeric operator, scalar numeric function, or aggregate
  reduction; and
- after a project binding returns its host scalar, before R018 checks its
  declared result contract.

Normalization precedes expression dispatch, missing handling, conversion,
comparison, equality, grouping, ordering, range selection, key validation,
verification, contract fingerprinting, and artifact rendering. None of those
operations can observe a non-finite float or fall back to host-runtime
semantics for one. They observe the missing value and apply their existing
missing-value behavior. In particular, a normalized output key fails R005's
non-missing key requirement, `not_missing` fails while verifications that skip
missing values skip it under R009, and a delimited artifact renders it as an
empty field under R005. No artifact or canonical value has an infinity or NaN
spelling.

The policy is value-based rather than a universal text sentinel. An unquoted
YAML scalar matching a core-schema non-finite form first resolves to a float
and is therefore normalized; quoting the same characters preserves a `str`.
A stored or quoted string remains text when its declared destination is `str`.
Only numeric parsing gives such text a numeric meaning, as defined below.

Normalization does not bypass a constraint that prohibits missing. For
example, a project binding that returns a non-finite float has returned missing
after normalization and is valid only when its R018 contract declares
`may_return_missing: true`.

## Conversion

Conversion applies the completed derivation result to the declared column type
at the point R005 defines. Conversion is deterministic, and a conversion that
is not defined below fails rather than producing a substitute value.

A table row is the runtime type of the value being converted and a table
column is the declared type:

| From | to `str` | to `int` | to `float` | to `date` | to `datetime` |
|---|---|---|---|---|---|
| missing | missing | missing | missing | missing | missing |
| `str` | identity | parse, then numeric to `int` | parse | R016 | R016 |
| `int` | decimal text | identity | widen | fail | fail |
| `float` | decimal text, see below | integral only | identity | fail | fail |
| `date` | R016 | fail | fail | identity | fail |
| `datetime` | R016 | fail | fail | fail | identity |
| `bool` | fail | fail | fail | fail | fail |

A missing value converts to missing in every type. Conversion is not attempted,
so `conversion_failure` does not fire for a missing input and a missing result
is not a failure.

Parsing a `str` to a number accepts R010's `number` production with an optional
leading `+` or `-` and no surrounding whitespace. It also recognizes exactly
the YAML 1.2 core-schema non-finite forms: `.inf`, `.Inf`, or `.INF` with an
optional leading `+` or `-`, and `.nan`, `.NaN`, or `.NAN`. A recognized
non-finite form is parsed as a float and immediately normalized to missing
before conversion continues. Any other text fails.

A cell reading **R016** applies that rule: the text a temporal value is parsed
from, the canonical text it is written back to, and the conversions it does
not permit are all stated there. Naming the rule rather than repeating its
grammar is what keeps the form a column conversion applies and the form any
other reader applies from drifting apart.

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

Conversion from `float` to `str` uses the shortest decimal text that parses
back to the same binary64 value, with a trailing `.0` omitted for an integral
value. Every float reaching this conversion is finite under the normalization
policy above. This conversion preserves the value; it is not display rounding.

Calculations, comparisons, verifications, and dependent derivations always use
the unrounded value. Final artifact display precision and its half-away-from-
zero rounding belong to the output-rendering rule and occur once, after
calculation. R018's conformance comparison similarly operates on a temporary
copy and never changes a value used by the specification.

## Errors

- A `column.type` outside `column_type`: schema failure under R006 `values`.
- A conversion listed as `fail`, or a parse that does not match: conversion
  failure, handled by `conversion_failure` under R008 and otherwise fatal
  under R005.
- A numeric value outside the 64-bit signed range converted to `int`: fail.
- A non-integral numeric value converted to `int`: fail.
- Reliance on an unresolved conversion: fail rather than choose a
  representation.
