---
id: R011
title: Column Type Vocabulary, Missing Normalization, and Conversion
status: normative
applies_to: [column.type, column_type, literal_value, derivation,
  conversion_failure]
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
this rule's normalization. R019 owns the contents, equality, order, and
normalization policy of `str`.

## Three type namespaces

**R011-1.** The word `type` appears in three roles, distinguished by position
rather than by name. No value is shared between their vocabularies except by
coincidence of spelling:

- Schema descriptor keyword: `type` inside a descriptor, in a class field or
  a value type. Vocabulary: R006 type expressions over `str`, `int`,
  `float`, `bool`, `"null"`, `list`, `dict`, and named types.
- Declared column type: the `type` field of a `column_class` entry in a
  specification. Vocabulary: `column_type`, below.
- Runtime value type: never written; the type a value carries during
  evaluation. Vocabulary: R007 type behavior.

**R011-2.** `column_class` declares a field whose own name is `type`, so its
declaration reads `- type: {type: column_type, required: true}`. The outer
`type` is a specification field name and the inner `type` is the R006
descriptor keyword; R006 resolves the two without ambiguity.

**R011-3.** The schema vocabulary and the column vocabulary are not the same
set. `str`, `int`, and `float` are spelled the same in both and mean the
same runtime values. `date` and `datetime` are column types and not schema
types. `bool`, `"null"`, `list`, and `dict` are schema types and not column
types.

## Closed column vocabulary

**R011-4.** `column_type` is closed. A column declares exactly one of:

- `str`: a text value defined by R019.
- `int`: a 64-bit signed integer, as defined by R010.
- `float`: an IEEE 754 binary64 value, as defined by R010.
- `date`: a calendar date, as defined by R016.
- `datetime`: a local civil datetime, as defined by R016.

**R011-5.** Every type additionally admits the missing value.

**R011-6.** `date` and `datetime` are the two temporal types, and R016
defines both: what each admits, the text it is read from and written back
to, how two of them order, and which operations read them. This rule adds
nothing to that definition. A value neither type admits is a `str` like any
other, and ISO 8601 text orders chronologically under R007 comparison.

**R011-7.** There is no Boolean column type; a flag is a `str` column with
an `allowed_values` verification, as the examples write it.

**R011-8.** Extending this vocabulary is a rule change, not an
implementation choice.

## Non-finite floats are missing

**R011-9.** A non-finite float is positive infinity, negative infinity, or
any NaN binary64 value. Every non-finite float is the missing value. It is
normalized immediately at every boundary where a float enters the language
or a numeric operation produces one:

**R011-10.** After YAML 1.2 core-schema scalar resolution in a
specification, schema, project environment, or conformance document, before
a literal or default is validated or used.

**R011-11.** After a self-describing source supplies a typed value or stored
text is parsed as a number.

**R011-12.** After a built-in expression, mapping, conditional, coalescing
operation, or handler selects or substitutes a result.

**R011-13.** After each numeric operator, scalar numeric function, or
aggregate reduction.

**R011-14.** After a project binding returns its host scalar, before R018
checks its declared result contract.

**R011-15.** Normalization precedes expression dispatch, missing handling,
conversion, comparison, equality, grouping, ordering, range selection, key
validation, verification, contract fingerprinting, and artifact rendering.
None of those operations can observe a non-finite float or fall back to
host-runtime semantics for one. They observe the missing value and apply
their existing missing-value behavior. In particular, a normalized output
key fails R005's non-missing key requirement, `not_missing` fails while
verifications that skip missing values skip it under R009, and an artifact
carries it as the missing value its profile writes under R020. No artifact
or canonical value has an infinity or NaN spelling.

**R011-16.** The policy is value-based rather than a universal text
sentinel. An unquoted YAML scalar matching a core-schema non-finite form
first resolves to a float and is therefore normalized; quoting the same
characters preserves a `str`. A stored or quoted string remains text when
its declared destination is `str`. Only numeric parsing gives such text a
numeric meaning, as defined below.

**R011-17.** Normalization does not bypass a constraint that prohibits
missing. For example, a project binding that returns a non-finite float has
returned missing after normalization and is valid only when its R018
contract declares `may_return_missing: true`.

## Conversion

**R011-18.** Conversion applies the completed derivation result to the
declared column type at the point R005 defines. Conversion is
deterministic, and a conversion that is not defined below fails rather than
producing a substitute value.

**R011-19.** A table row is the runtime type of the value being converted
and a table column is the declared type:

- missing converts to missing in every type.
- `str` converts to `str` by R019 identity; to `int` by parsing, then
  numeric to `int`; to `float` by parsing; to `date` and `datetime` by
  R016.
- `int` converts to `str` as decimal text; to `int` by identity; to
  `float` by widening; to `date` and `datetime` by failing.
- `float` converts to `str` as decimal text (see float-to-text below); to
  `int` when integral only; to `float` by identity; to `date` and
  `datetime` by failing.
- `date` converts to `str` by R016; to `date` by identity; to `int`,
  `float`, and `datetime` by failing.
- `datetime` converts to `str` by R016; to `datetime` by identity; to
  `int`, `float`, and `date` by failing.
- `bool` fails in every type.

**R011-20.** A missing value converts to missing in every type. Conversion
is not attempted, so `conversion_failure` does not fire for a missing input
and a missing result is not a failure.

**R011-21.** Parsing a `str` to a number accepts R010's `number` production
with an optional leading `+` or `-` and no surrounding whitespace. It also
recognizes exactly the YAML 1.2 core-schema non-finite forms: `.inf`,
`.Inf`, or `.INF` with an optional leading `+` or `-`, and `.nan`, `.NaN`,
or `.NAN`. A recognized non-finite form is parsed as a float and
immediately normalized to missing before conversion continues. Any other
text fails.

**R011-22.** A cell reading **R016** applies that rule: the text a temporal
value is parsed from, the canonical text it is written back to, and the
conversions it does not permit are all stated there. The collected precision
a temporal value carries is stated there too, and canonical text carries the
fields alone, so a `date` or `datetime` converted to `str` does not carry
it. Naming the rule rather than repeating its grammar is what keeps the form
a column conversion applies and the form any other reader applies from
drifting apart.

**R011-23.** Converting a numeric value to `int` succeeds only when the
value is exactly integral and within the 64-bit signed range. A non-integral
value fails; it is neither truncated nor rounded. Write `FLOOR`, `CEIL`, or
`TRUNC` in the `compute` expression to choose the integer explicitly.

**R011-24.** Widening an `int` to `float` is exact for magnitudes below 2^53
and is otherwise the nearest binary64 value.

**R011-25.** A `bool` never converts. No column type is Boolean, but
`literal_value` admits `true` and `false`, so a Boolean value can reach
conversion. Failing is the conservative reading: it is deterministic, no
example depends on any other outcome, and a later rule may define a mapping
without invalidating a specification written under this one.

## Float to text

**R011-26.** Conversion from `float` to `str` uses the shortest sequence of
significant decimal digits that parses back to the same binary64 value,
written in positional notation with a trailing `.0` omitted for an integral
value. Every float reaching this conversion is finite under the
normalization policy above. This conversion preserves the value; it is not
display rounding.

**R011-27.** The text never carries an exponent, so one value has exactly
one spelling: `0.0001` and not `1e-4`, and a large magnitude is written out
in full. Shortest selects the digits, not the characters. R020 writes this
same text into an artifact and states the bytes two runtimes must agree on,
which they can only do if one value has one text.

**R011-28.** Calculations, comparisons, verifications, and dependent
derivations always use the unrounded value. Final artifact display precision
and its half-away-from-zero rounding belong to R020's `output.decimals` and
occur once, when a field is written. R018's conformance comparison similarly
operates on a temporary copy and never changes a value used by the
specification.

## Rationale

Normalizing every non-finite float to missing at the boundary where it
enters keeps host floating-point semantics from leaking into the language:
no downstream operation can observe an infinity or NaN, so none needs its
own fallback for one. The policy is value-based rather than a text sentinel
so that quoting still preserves text and only numeric parsing gives such
text a numeric meaning. Failing an undefined conversion instead of choosing
a representation keeps the type system conservative -- a later rule can
define a mapping without invalidating specifications written under this
one -- and shortest-round-trip float text gives one value one spelling so
that two runtimes can agree on the bytes an artifact carries.

## Errors

**R011-29.** A `column.type` outside `column_type`: schema failure under
R006 `values`.

**R011-30.** A conversion listed as `fail`, or a parse that does not match:
conversion failure, handled by `conversion_failure` under R008 and
otherwise fatal under R005.

**R011-31.** A numeric value outside the 64-bit signed range converted to
`int`: fail.

**R011-32.** A non-integral numeric value converted to `int`: fail.

**R011-33.** Reliance on an unresolved conversion: fail rather than choose a
representation.
