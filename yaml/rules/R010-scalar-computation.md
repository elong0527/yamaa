---
id: R010
title: Scalar Numeric Computation
status: normative
applies_to: [expression.compute, numeric_expression]
depends_on: [R001, R004, R005, R006, R007, R011, R014, R015, R017]
---

# Scalar numeric computation

## Intent

Express arithmetic that combines several columns as one readable formula,
without a registry entry per operator and without host-language code.

## Boundaries

This rule owns the `numeric_expression` primitive: its grammar, numeric types,
missing-value behavior, and failure conditions. R017 owns the portable function
vocabulary and its machine-readable call contracts.
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

`compute` evaluates a closed numeric grammar over current-output columns,
fields of a declared record lookup, and numeric literals, and returns one
numeric value per current row. The grammar is a subset of SQL.

## Identifiers

An identifier resolves the way the `sql` primitive already resolves one in the
same phase, so a formula and a predicate never disagree about a name.

During column derivation an unqualified identifier is a current-output column.
A qualified identifier is permitted only when its qualifier is a declared R015
record lookup `id`; it reads the named field of that lookup's selected record.
An arbitrary `DATASET.VARIABLE` reference is not permitted: bind the source
variable to a column first and compute from it. Omitting that binding column
from `output.columns` keeps it out of the final dataset.

During row construction an identifier is either a variable of the row driver,
qualified exactly as `row.filter` qualifies one, or an unqualified column
derived by the same `rows` entry. No other dataset may be qualified, because
row construction precedes the R003 join and sees only the row driver.

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

An identifier that does not resolve in its phase is an error. A lookup field
carries the type R014 assigns to the field in the lookup's dataset. R001
collects these identifiers and the lookup's source dependencies, so a
`compute` derivation participates in dependency ordering exactly as a
predicate does.

## Grammar

```text
expr    := term (("+" | "-") term)*
term    := factor (("*" | "/") factor)*
factor  := ("-" | "+")? primary
primary := number | "NULL" | identifier | call | "(" expr ")"
identifier := name ["." name]
call    := portable_name "(" [expr ("," expr)*] ")"
portable_name := name | namespace "::" name
number  := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

Precedence is unary sign, then `*` and `/`, then binary `+` and `-`, all
left-associative. Parentheses override precedence. Function names and `NULL`
are case-insensitive; namespaces and identifiers are not.

## Portable scalar calls

A call resolves through R017. The resolved entry must have
`evaluation_kind: scalar`, accept the static type of every argument, and return
a numeric type. Core calls are unqualified. A namespaced call requires the
exact extension declaration R017 defines.

The generated table in `registry/README.md` is the readable inventory; the
schema entry point's `portable_registry` file is the source of truth for names,
aliases, arity, parameter types, promotion, result type, missing values,
failures, determinism, accuracy, and availability. A function name outside
that registry, an aggregate function, any operator outside the grammar, any
string literal, comparison or Boolean operator, `CASE`, window function or
`OVER`, subquery, or host-language call is a validation error.

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
- A function call uses its registry entry's `type_promotion` and
  `result_type`. An `always_float` result is a `float`; a
  `promoted_numeric` result is `int` only when every non-missing argument is
  `int`, and is otherwise `float`.

`GREATEST` and `LEAST` stay numeric here like every other function in this
grammar. A row-wise extreme over dates, or over any other comparable type, is
the `greatest` and `least` registry expressions that R007 defines; this
grammar is not widened to reach them.

An identifier whose runtime type is neither `int` nor `float` is an error.
R007 already forbids implicit conversion between operation inputs, and this
rule does not relax that: a collected string is converted by binding it to a
numeric column first.

## Missing values

`NULL` propagates through every operator. A function applies the
`missing_values` behavior in its registry entry, so an exception to propagation
is explicit and shared by both runtimes.

A `compute` derivation therefore needs no guarding predicate to survive a
missing input, and a formula that must yield missing rather than fail says so
with `NULLIF`. Percentage change against a zero base is
`100 * (VALUE - BASE) / NULLIF(BASE, 0)`.

## Failure conditions

These fail the run. They are not silently converted to missing, consistent with
R005: an implementation must not replace an error with a missing value.

- Division by zero by `/`. Write `NULLIF(denominator, 0)` to choose missing
  explicitly.
- Integer overflow of `+`, `-`, or `*` under `int` promotion.
- A float result that is infinite or not a number.

A function call additionally applies the domain, overflow, and non-finite
result behavior declared by its registry entry.

Floating-point results are not exact decimals. `POWER(x, 2)` and `x * x` are
permitted to differ in the last place. A specification cannot round that away,
so a derivation that needs a stable decimal must be written as the formula that
produces one.

## Determinism

Evaluation must be deterministic and free of side effects. Implementations must
meet each function entry's cross-runtime accuracy contract and produce the same
serialized values for every example. Two consequences are not optional:

- `/` never truncates. Language or engine settings that make division integral
  must be overridden.
- Evaluation follows the written association exactly. Implementations must not
  reassociate, redistribute, or algebraically simplify an expression, and must
  not enable fast-math or optimizer rewrites that do. `a / (b * b)` and
  `a / b / b` are different formulas and may return different doubles; both are
  correct, and an implementation must return the one that was written.

## Errors

- A `numeric_expression` that does not parse under the grammar: fail.
- A function call that fails R017 name, kind, version, arity, or type
  validation: fail with R017's stable condition.
- An aggregate, window, comparison, Boolean, conditional, string, or
  host-language construct: fail.
- A qualified identifier whose qualifier is not a declared record lookup
  during column derivation: fail.
- An identifier that does not resolve to a declared output column or a field
  of a declared record lookup: fail.
- An identifier whose runtime type is not numeric: fail.
- Any failure condition listed above: fail, reporting the expression and the
  operation that failed.
