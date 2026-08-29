---
id: R010
title: Scalar Numeric Computation
status: normative
applies_to: [expression.compute, numeric_expression]
depends_on: [R001, R004, R005, R006, R007, R011]
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
their type or their permitted vocabulary.

## Scope

`compute` evaluates a closed numeric grammar over current-output columns and
numeric literals and returns one numeric value per current row. The grammar is
a subset of SQL.

## Identifiers

An identifier resolves the way the `sql` primitive already resolves one in the
same phase, so a formula and a predicate never disagree about a name.

During column derivation an identifier is an unqualified current-output column.
A qualified `DATASET.VARIABLE` reference is not permitted there: bind the source
variable to a column first and compute from it. `output: false` keeps that
binding column out of the final dataset.

During row construction an identifier is either a variable of the row driver,
qualified exactly as `row.filter` qualifies one, or an unqualified column
derived by the same `rows` entry. No other dataset may be qualified, because
row construction precedes the R003 join and sees only the row driver.

```yaml
- name: HEIGHTCM
  type: float
  output: false
  derivation:
    source: DM.HEIGHTCM
- name: BMI
  type: float
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"
```

An identifier that does not resolve in its phase is an error. R001 collects
these identifiers, so a `compute` derivation participates in dependency
ordering exactly as a predicate does.

## Grammar

```text
expr    := term (("+" | "-") term)*
term    := factor (("*" | "/") factor)*
factor  := ("-" | "+")? primary
primary := number | "NULL" | identifier | call | "(" expr ")"
identifier := name ["." name]
call    := function "(" [expr ("," expr)*] ")"
number  := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

Precedence is unary sign, then `*` and `/`, then binary `+` and `-`, all
left-associative. Parentheses override precedence. Function names and `NULL`
are case-insensitive; identifiers are not.

Permitted functions are exactly:

| Function | Result |
|---|---|
| `ABS(x)` | absolute value |
| `CEIL(x)` | least integer value not less than `x` |
| `FLOOR(x)` | greatest integer value not greater than `x` |
| `TRUNC(x)` | `x` with its fractional part removed, toward zero |
| `SQRT(x)` | non-negative square root |
| `POWER(x, y)` | `x` raised to `y` |
| `EXP(x)` | `e` raised to `x` |
| `LN(x)` | natural logarithm |
| `MOD(x, y)` | remainder of `x / y`, taking the sign of `x` |
| `GREATEST(x, ...)` | largest non-`NULL` argument, or `NULL` if all are `NULL` |
| `LEAST(x, ...)` | smallest non-`NULL` argument, or `NULL` if all are `NULL` |
| `NULLIF(x, y)` | `NULL` when `x = y`, otherwise `x` |
| `COALESCE(x, ...)` | first non-`NULL` argument, or `NULL` if all are `NULL` |

`GREATEST` and `LEAST` require at least two arguments; `COALESCE` requires at
least one. Any other function name, any operator outside the grammar, any
string literal, any comparison or Boolean operator, any `CASE`, any aggregate
function, any window function or `OVER`, any subquery, and any host-language
call are validation errors. Closing the vocabulary is what makes portability
checkable; widening it requires amending this table.

`LOG` is excluded because its base differs between dialects. Write `LN(x)` or
`LN(x) / LN(b)`.

### There is no rounding function

**A derivation must not round.** `ROUND` is absent, not merely discouraged, and
a specification cannot round a value at all. Analysis datasets carry the
computed value at full precision and the number of places shown is decided when
the value is reported, so a rounding function here could only be misused. It
would also be a portability hazard: R, Python, and `numpy` round half to even
while SAS rounds half away from zero, so any rounding inheriting the host
language disagrees across runtimes on exactly the values a reviewer checks.
R011 keeps the same position at conversion, where a non-integral value fails
rather than being truncated.

`CEIL`, `FLOOR`, and `TRUNC` remain. They are not presentation rounding: they
return an integral part exactly, with no mode to choose, and `FLOOR(a / b)` is
how this grammar expresses integer division.

## Types

`int` is a 64-bit signed integer. `float` is IEEE 754 binary64.

- `+`, `-`, `*`: `int` with `int` returns `int`; any `float` operand returns
  `float`.
- `/`: always returns `float`. There is no integer division. Write
  `FLOOR(a / b)` for a floor-divided integer.
- `SQRT`, `POWER`, `EXP`, and `LN` return `float`.
- `CEIL`, `FLOOR`, and `TRUNC` return `float`. Declare the column
  `type: int` when an integer is wanted; R005 converts the completed result
  and R011 defines that conversion.
- `ABS`, `GREATEST`, `LEAST`, `MOD`, `NULLIF`, and `COALESCE` return the
  promoted type of their arguments: `int` when every argument is `int`,
  otherwise `float`.

`GREATEST` and `LEAST` stay numeric here like every other function in this
grammar. A row-wise extreme over dates, or over any other comparable type, is
the `greatest` and `least` registry expressions that R007 defines; this
grammar is not widened to reach them.

An identifier whose runtime type is neither `int` nor `float` is an error.
R007 already forbids implicit conversion between operation inputs, and this
rule does not relax that: a collected string is converted by binding it to a
numeric column first.

## Missing values

`NULL` propagates. Any operator or function argument that is `NULL` produces a
`NULL` result, except `COALESCE`, `NULLIF`, `GREATEST`, and `LEAST`, whose
argument-level behavior is defined in the table above.

A `compute` derivation therefore needs no guarding predicate to survive a
missing input, and a formula that must yield missing rather than fail says so
with `NULLIF`. Percentage change against a zero base is
`100 * (VALUE - BASE) / NULLIF(BASE, 0)`.

## Failure conditions

These fail the run. They are not silently converted to missing, consistent with
R005: an implementation must not replace an error with a missing value.

- Division by zero, by `/` or by `MOD`. Write `NULLIF(denominator, 0)` to
  choose missing explicitly.
- `SQRT` of a negative argument.
- `LN` of a zero or negative argument.
- `POWER` with a zero base and a negative exponent, or a negative base and a
  non-integer exponent.
- Integer overflow of `+`, `-`, or `*` under `int` promotion.
- A float result that is infinite or not a number.

Floating-point results are not exact decimals. `POWER(x, 2)` and `x * x` are
permitted to differ in the last place. A specification cannot round that away,
so a derivation that needs a stable decimal must be written as the formula that
produces one.

## Determinism

Evaluation must be deterministic and free of side effects. Implementations must
produce identical results in R and Python for every example. Two consequences
are not optional:

- `/` never truncates. Language or engine settings that make division integral
  must be overridden.
- Evaluation follows the written association exactly. Implementations must not
  reassociate, redistribute, or algebraically simplify an expression, and must
  not enable fast-math or optimizer rewrites that do. `a / (b * b)` and
  `a / b / b` are different formulas and may return different doubles; both are
  correct, and an implementation must return the one that was written.

## Errors

- A `numeric_expression` that does not parse under the grammar: fail.
- A function name outside the permitted table, or called with a prohibited
  argument count: fail.
- An aggregate, window, comparison, Boolean, conditional, string, or
  host-language construct: fail.
- A qualified `DATASET.VARIABLE` identifier: fail.
- An identifier that does not resolve to a declared output column: fail.
- An identifier whose runtime type is not numeric: fail.
- Any failure condition listed above: fail, reporting the expression and the
  operation that failed.
