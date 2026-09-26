---
id: operations/computation
title: Numeric computation
status: normative
---

# Numeric computation

## Requirements

### Type behavior

<a id="req-0306"></a>

**REQ-0306.** `cut` requires a numeric source.

<a id="req-0307"></a>

**REQ-0307.** `compute` requires every identifier in its expression to be
numeric.

### Scope

<a id="req-0407"></a>

**REQ-0407.** `compute` evaluates a closed numeric grammar over constructed output
columns, fields of a declared record lookup, and numeric literals.
`compute` returns one numeric value per current row. The grammar is a
subset of SQL. [Numeric values](../values/numbers.md)'s representation,
promotion, and overflow rules apply to every operator and function.

### Identifiers

<a id="req-0408"></a>

**REQ-0408.** An identifier uses the `predicate` primitive's resolution in the same
phase. A formula and a predicate never disagree about a name.

<a id="req-0409"></a>

**REQ-0409.** During column derivation an unqualified identifier is a
constructed output column. A qualified identifier is permitted only when its
qualifier is a declared [Lookup and joins](lookup.md) record lookup `id`. The qualified identifier
reads the named field of that lookup's selected record. An arbitrary
`DATASET.VARIABLE` reference is not permitted: bind the source variable
to a column first and compute from that column. Omitting that binding
column from `output.columns` keeps the column out of the final dataset.

<a id="req-0410"></a>

**REQ-0410.** During ungrouped row construction an identifier is either a
variable of the row template's input dataset, qualified exactly as
`row.filter` qualifies a variable, or an unqualified column derived by
the same `rows` entry. A value from another dataset reaches the formula
only through such a column, bound by a row-phase source or lookup under
[REQ-0156](lookup.md#req-0156).

<a id="req-0411"></a>

**REQ-0411.** During grouped row construction an identifier qualified to
the row template's input dataset must be one of the enclosing
`row.group_by` variables. Other values are first reduced to a row-derived
column with `aggregate`. A value from another dataset is first bound to
a row-derived column with a source or lookup under [REQ-0157](lookup.md#req-0157); the formula
itself qualifies no dataset but its own.

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

<a id="req-0412"></a>

**REQ-0412.** An identifier that does not resolve in its phase is an error. A
lookup field carries the type [Source ingestion](../storage/ingestion.md) assigns to the field in the lookup's
dataset. [Execution lifecycle](../execution/lifecycle.md) collects these identifiers and lookup source dependencies. A
`compute` derivation therefore participates in dependency ordering like a
predicate does.

### Grammar

<a id="req-0413"></a>

**REQ-0413.** The grammar is:

```text
expr       := term (("+" | "-") term)*
term       := factor (("*" | "/") factor)*
factor     := ("-" | "+")? primary
primary    := number | "NULL" | identifier | call | "(" expr ")"
identifier := name ["." name]
call       := function "(" [expr ("," expr)*] ")"
number     := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

`grammar/numeric.yaml` is this grammar's single source. The block above
renders the grammar file. The file vocabulary closes the function table below.
The file cases record the text every implementation must accept or reject,
the identifiers an accepted text binds, and the resulting parse.
Repository validation and the R implementation both read that file, so no
transcription of this grammar can drift from that file without failing.

<a id="req-0414"></a>

**REQ-0414.** Precedence is unary sign, then `*` and `/`, then binary `+` and
`-`, all left-associative. Parentheses override precedence. Function names
and `NULL` are case-insensitive; identifiers are not.

<a id="req-0415"></a>

**REQ-0415.** Permitted functions are exactly:

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

<a id="req-0416"></a>

**REQ-0416.** `GREATEST` and `LEAST` require at least two arguments;
`COALESCE` requires at least one. Any other function name, any operator
outside the grammar, any string literal, any comparison or Boolean operator,
any `CASE`, any aggregate function, any window function or `OVER`, any
subquery, and any host-language call are validation errors. Widening the
vocabulary requires amending the table in [REQ-0415](computation.md#req-0415).

<a id="req-0417"></a>

**REQ-0417.** `LOG` is excluded. Write `LN(x)` or `LN(x) / LN(b)`.

#### One rounding with fixed tie behavior

<a id="req-0418"></a>

**REQ-0418.** `round_half_away_from_zero` is the one rounding the language
admits, and its tie behavior is fixed: a value exactly halfway between two
candidates, or within `sqrt(2^-52) * 10^-digits` below such a tie, rounds
half away from zero. No other
rounding exists: the `compute` grammar gains no `ROUND`, and a derivation
must not round by any other spelling. The source must be numeric; a
non-numeric source is an `incompatible_input_type` validation error. Missing
stays missing. A value that rounds to zero returns positive zero, never
negative zero. Analysis datasets otherwise carry computed values at full
precision; reporting decides the displayed places. [Types and
conversion](../values/types.md) has the same rule at conversion, where a
non-integral value fails rather than being truncated.

<a id="req-0419"></a>

**REQ-0419.** `CEIL`, `FLOOR`, and `TRUNC` remain. They are not presentation
rounding. They return an integral part, with no mode to choose.
`FLOOR(a / b)` is how this grammar expresses integer division.

### Types

<a id="req-0425"></a>

**REQ-0425.** `GREATEST` and `LEAST` stay numeric like every other
function in this grammar. A row-wise extreme over dates, or over any other
comparable type, is the `greatest` and `least` registry expressions that
[Expression evaluation](expressions.md) defines. This grammar is not widened to reach them.

<a id="req-0426"></a>

**REQ-0426.** An identifier whose runtime type is neither `int` nor `float`
is an error. [REQ-0004](../values/types.md#req-0004) forbids implicit conversion between operation
inputs, and this contract does not relax that rule. A collected string is
converted by binding it to a numeric column first.

### Missing values

<a id="req-0427"></a>

**REQ-0427.** `NULL` propagates. A `NULL` argument to an operator or function
produces a `NULL` result, except `COALESCE`, `NULLIF`, `GREATEST`, and `LEAST`,
whose argument-level behavior is defined in the [REQ-0415](computation.md#req-0415) table.

<a id="req-0428"></a>

**REQ-0428.** A `compute` derivation therefore needs no guarding predicate to
survive a missing input. A formula that must yield missing rather than
fail uses `NULLIF`. Percentage change against a zero base is
`100 * (VALUE - BASE) / NULLIF(BASE, 0)`.

### Failure conditions

<a id="req-0429"></a>

**REQ-0429.** The following conditions fail the run. Consistent with [Execution lifecycle](../execution/lifecycle.md),
implementations must not silently convert a failure to missing.

<a id="req-0430"></a>

**REQ-0430.** Division by zero, by `/` or by `MOD`. Write
`NULLIF(denominator, 0)` to choose missing explicitly.

<a id="req-0431"></a>

**REQ-0431.** `SQRT` of a negative argument.

<a id="req-0432"></a>

**REQ-0432.** `LN` of a zero or negative argument.

<a id="req-0433"></a>

**REQ-0433.** `POWER` with a zero base and a negative exponent, or a negative
base and a non-integer exponent.

### Determinism

<a id="req-0436"></a>

**REQ-0436.** Evaluation must be deterministic and free of side effects.
Implementations must produce identical results in R and Python for every
example.

<a id="req-0437"></a>

**REQ-0437.** `/` never truncates. Language or engine settings that make
division integral must be overridden.

<a id="req-0438"></a>

**REQ-0438.** Evaluation follows the written association. An
implementation must not reassociate, redistribute, or algebraically
simplify an expression, and must not switch on a numeric performance
option that does any of these. `a / (b * b)` and `a / b / b` are different
formulas and may return different doubles. Both are correct. An
implementation returns the double for the written formula.

### Interface behavior

<a id="req-1118"></a>

**REQ-1118.** The `expressions.cut` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `expressions.cut.source` | Numeric variable to classify. |
| `expressions.cut.breaks` | Ascending interval boundaries. |
| `expressions.cut.labels` | One label per interval; exactly one more than breaks. |
| `expressions.cut.right` | Right-closed/left-open; false is left-closed/right-open. |
| `expressions.cut.missing` | Value returned when source is missing. |
| `Result` | Assigns a numeric source to one of the labeled break intervals. |

<a id="req-1119"></a>

**REQ-1119.** The `expressions.compute` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `expressions.compute.expr` | Closed numeric expression over named variables. |
| `Result` | Evaluates one scalar numeric formula under [Numeric computation](computation.md). |

<a id="req-1120"></a>

**REQ-1120.** The `numeric_expression` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `numeric_expression` | Numeric formula in the portable grammar defined by [Numeric computation](computation.md). |

<a id="req-1121"></a>

**REQ-1121.** The `module` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `Scope` | [Numeric computation](computation.md) defines the closed numeric grammar, types, and failure conditions. |

<a id="req-1172"></a>

**REQ-1172.** The `expressions.round_half_away_from_zero` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `expressions.round_half_away_from_zero.source` | Numeric variable to round. |
| `expressions.round_half_away_from_zero.digits` | Integer decimal places; negative rounds left of the decimal point. |
| `Result` | The source rounded to `digits` places with ties half away from zero; a float. |

## Error conditions

<a id="req-0439"></a>

**REQ-0439.** A `numeric_expression` that does not parse under the grammar:
fail.

<a id="req-0440"></a>

**REQ-0440.** A function name outside the permitted table, or called with a
prohibited argument count: fail.

<a id="req-0441"></a>

**REQ-0441.** An aggregate, window, comparison, Boolean, conditional, string,
or host-language construct: fail.

<a id="req-0442"></a>

**REQ-0442.** A qualified identifier whose qualifier is not a declared record
lookup during column derivation: fail.

<a id="req-0443"></a>

**REQ-0443.** An identifier that does not resolve to a declared output column
or a field of a declared record lookup: fail.

<a id="req-0444"></a>

**REQ-0444.** An identifier whose runtime type is not numeric: fail.

<a id="req-0445"></a>

**REQ-0445.** Any failure condition listed above: fail, reporting the
expression and the operation that failed.
