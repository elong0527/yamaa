---
id: R010
title: Scalar Numeric Computation
status: normative
applies_to: [expression.compute, numeric_expression]
---

# Scalar numeric computation

## Intent

Express arithmetic that combines several columns as one readable formula,
without a registry entry per operator and without host-language code.

## Boundaries

This rule owns the `numeric_expression` primitive: its grammar, function
vocabulary, numeric types, missing-value behavior, and failure conditions.
`compute` is the only arithmetic expression and is deliberately numeric.
Strings, dates, comparison, conditional selection, and row-wise extremes over
non-numeric types keep their registered expressions under R007, so a general
expression string cannot displace the typed registry. The Boolean-valued `sql`
primitive is R004; the two share notation and identifier resolution but not
their type or their permitted vocabulary. Reduction over many records is
R013's `aggregate_expression`, which reuses this grammar's operators,
functions, numeric types, and failure conditions; this rule stays per-row and
admits no reduction.

## Scope

**R010-1.** `compute` evaluates a closed numeric grammar over current-output
columns, fields of a declared record lookup, and numeric literals, and
returns one numeric value per current row. The grammar is a subset of SQL.

## Identifiers

**R010-2.** An identifier resolves the way the `sql` primitive already
resolves one in the same phase, so a formula and a predicate never disagree
about a name.

**R010-3.** During column derivation an unqualified identifier is a
current-output column. A qualified identifier is permitted only when its
qualifier is a declared R015 record lookup `id`; it reads the named field of
that lookup's selected record. An arbitrary `DATASET.VARIABLE` reference is
not permitted: bind the source variable to a column first and compute from
it. Omitting that binding column from `output.columns` keeps it out of the
final dataset.

**R010-4.** During ungrouped row construction an identifier is either a
variable of the row driver, qualified exactly as `row.filter` qualifies one,
or an unqualified column derived by the same `rows` entry.

**R010-5.** During grouped row construction a qualified driver identifier
must be one of the enclosing `row.group_by` variables; other values are
first reduced to a row-derived column with `aggregate`. No other dataset may
be qualified, because row construction precedes the R003 join and sees only
the row driver.

```yaml
- name: HEIGHTCM
  type: float
  derivation:
    source: DM.HEIGHTCM
- name: BMI
  type: float
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"
```

**R010-6.** An identifier that does not resolve in its phase is an error. A
lookup field carries the type R014 assigns to the field in the lookup's
dataset. R001 collects these identifiers and the lookup's source
dependencies, so a `compute` derivation participates in dependency ordering
exactly as a predicate does.

## Grammar

**R010-7.** The grammar is:

```text
expr       := term (("+" | "-") term)*
term       := factor (("*" | "/") factor)*
factor     := ("-" | "+")? primary
primary    := number | "NULL" | identifier | call | "(" expr ")"
identifier := name ["." name]
call       := function "(" [expr ("," expr)*] ")"
number     := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

`grammar/numeric.yaml` is this grammar's single source. The block above is
its rendering, its vocabulary closes the function table below, and its cases
record the text every implementation must accept or reject, the identifiers
an accepted text binds, and the parse it produces. Repository validation and
the R implementation both read that file, so no transcription of this
grammar can drift from it without failing.

**R010-8.** Precedence is unary sign, then `*` and `/`, then binary `+` and
`-`, all left-associative. Parentheses override precedence. Function names
and `NULL` are case-insensitive; identifiers are not.

**R010-9.** Permitted functions are exactly:

- `ABS(x)`: absolute value.
- `CEIL(x)`: least integer value not less than `x`.
- `FLOOR(x)`: greatest integer value not greater than `x`.
- `TRUNC(x)`: `x` with its fractional part removed, toward zero.
- `SQRT(x)`: non-negative square root.
- `POWER(x, y)`: `x` raised to `y`.
- `EXP(x)`: `e` raised to `x`.
- `LN(x)`: natural logarithm.
- `MOD(x, y)`: remainder of `x / y`, taking the sign of `x`.
- `GREATEST(x, ...)`: largest non-`NULL` argument, or `NULL` if all are
  `NULL`.
- `LEAST(x, ...)`: smallest non-`NULL` argument, or `NULL` if all are
  `NULL`.
- `NULLIF(x, y)`: `NULL` when `x = y`, otherwise `x`.
- `COALESCE(x, ...)`: first non-`NULL` argument, or `NULL` if all are
  `NULL`.

**R010-10.** `GREATEST` and `LEAST` require at least two arguments;
`COALESCE` requires at least one. Any other function name, any operator
outside the grammar, any string literal, any comparison or Boolean operator,
any `CASE`, any aggregate function, any window function or `OVER`, any
subquery, and any host-language call are validation errors. Widening the
vocabulary requires amending the table in R010-9.

**R010-11.** `LOG` is excluded because its base differs between dialects.
Write `LN(x)` or `LN(x) / LN(b)`.

### There is no rounding function

**R010-12.** A derivation must not round. `ROUND` is absent, not merely
discouraged, and a specification cannot round a value at all. Analysis
datasets carry the computed value at full precision and the number of places
shown is decided when the value is reported. R011 keeps the same position at
conversion, where a non-integral value fails rather than being truncated.

**R010-13.** `CEIL`, `FLOOR`, and `TRUNC` remain. They are not presentation
rounding: they return an integral part exactly, with no mode to choose, and
`FLOOR(a / b)` is how this grammar expresses integer division.

## Types

**R010-14.** `int` is a 64-bit signed integer. `float` is IEEE 754 binary64.

**R010-15.** `+`, `-`, `*`: `int` with `int` returns `int`; any `float`
operand returns `float`.

**R010-16.** `/` always returns `float`. There is no integer division. Write
`FLOOR(a / b)` for a floor-divided integer.

**R010-17.** `SQRT`, `POWER`, `EXP`, and `LN` return `float`.

**R010-18.** `CEIL`, `FLOOR`, and `TRUNC` return `float`. Declare the column
`type: int` when an integer is wanted; R005 converts the completed result
and R011 defines that conversion.

**R010-19.** `ABS`, `GREATEST`, `LEAST`, `MOD`, `NULLIF`, and `COALESCE`
return the promoted type of their arguments: `int` when every argument is
`int`, otherwise `float`.

**R010-20.** `GREATEST` and `LEAST` stay numeric here like every other
function in this grammar. A row-wise extreme over dates, or over any other
comparable type, is the `greatest` and `least` registry expressions that
R007 defines; this grammar is not widened to reach them.

**R010-21.** An identifier whose runtime type is neither `int` nor `float`
is an error. R007 already forbids implicit conversion between operation
inputs, and this rule does not relax that: a collected string is converted
by binding it to a numeric column first.

## Missing values

**R010-22.** `NULL` propagates. Any operator or function argument that is
`NULL` produces a `NULL` result, except `COALESCE`, `NULLIF`, `GREATEST`,
and `LEAST`, whose argument-level behavior is defined in the table in
R010-9.

**R010-23.** A `compute` derivation therefore needs no guarding predicate to
survive a missing input, and a formula that must yield missing rather than
fail says so with `NULLIF`. Percentage change against a zero base is
`100 * (VALUE - BASE) / NULLIF(BASE, 0)`.

**R010-24.** R011's non-finite normalization applies after every numeric
operator or function and before the result is used by another part of the
expression.

## Failure conditions

**R010-25.** These fail the run. They are not silently converted to missing,
consistent with R005: an implementation must not replace an error with a
missing value.

**R010-26.** Division by zero, by `/` or by `MOD`. Write
`NULLIF(denominator, 0)` to choose missing explicitly.

**R010-27.** `SQRT` of a negative argument.

**R010-28.** `LN` of a zero or negative argument.

**R010-29.** `POWER` with a zero base and a negative exponent, or a negative
base and a non-integer exponent.

**R010-30.** Integer overflow of `+`, `-`, or `*` under `int` promotion.

**R010-31.** Floating-point results are not exact decimals. `POWER(x, 2)`
and `x * x` are permitted to differ in the last place. A specification
cannot round that away, so a derivation that needs a stable decimal must be
written as the formula that produces one.

## Determinism

**R010-32.** Evaluation must be deterministic and free of side effects.
Implementations must produce identical results in R and Python for every
example.

**R010-33.** `/` never truncates. Language or engine settings that make
division integral must be overridden.

**R010-34.** Evaluation follows the written association exactly.
Implementations must not reassociate, redistribute, or algebraically
simplify an expression, and must not enable fast-math or optimizer rewrites
that do. `a / (b * b)` and `a / b / b` are different formulas and may return
different doubles; both are correct, and an implementation must return the
one that was written.

## Rationale

Closing the vocabulary to one table keeps portability checkable: anything
outside the grammar fails validation instead of inheriting a host dialect.
There is deliberately no rounding function, because R, Python with `numpy`,
and SAS disagree on exactly the half-way values a reviewer checks, so any
rounding inherited from the host would disagree across runtimes. Carrying
full precision through the derivation and deciding display places at
reporting time avoids that hazard entirely. Fixing association and
forbidding reassociation serves the same goal: two formulas that differ only
in parenthesization may return different doubles, and each implementation
returns the one that was written.

## Errors

**R010-35.** A `numeric_expression` that does not parse under the grammar:
fail.

**R010-36.** A function name outside the permitted table, or called with a
prohibited argument count: fail.

**R010-37.** An aggregate, window, comparison, Boolean, conditional, string,
or host-language construct: fail.

**R010-38.** A qualified identifier whose qualifier is not a declared record
lookup during column derivation: fail.

**R010-39.** An identifier that does not resolve to a declared output column
or a field of a declared record lookup: fail.

**R010-40.** An identifier whose runtime type is not numeric: fail.

**R010-41.** Any failure condition listed above: fail, reporting the
expression and the operation that failed.
