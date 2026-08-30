---
id: R009
title: Verifications
status: draft
applies_to: [root.verifications, column.verifications, column_verifications, dataset_verifications]
depends_on: [R004, R005, R006, R011]
---

# Verifications

## Intent

Define closed, portable assertions over completed output values without using a
generic function or argument bag.

## Boundaries

This rule owns what each verification asserts, when it runs, and how a failure
is reported. R005 owns key uniqueness, which is checked by the output contract
rather than declared as a verification, and R004 owns the predicates that
`implies` and `predicate` evaluate.

Verifications reach across rows only in fixed ways, deliberately. `unique` and
`row_count` ask one question about the output as a whole; `all_or_none`,
`implies`, and `predicate` see one completed output row at a time. None of
them compares rows in an order, so an assertion over an ordered series — that
no partial response ever follows a complete response for a subject — is not a
shape this rule has. Stating one is a derivation followed by a row-wise
assertion: `row_value` under R007 places another row's value on the row, and
`predicate` compares the two.
`negative-adrs-partial-response-after-complete-response` does that against the
immediately preceding assessment, so it rejects a partial response adjacent to
a complete one and passes the same fault with an assessment in between.
Whether the vocabulary should gain an assertion over a frame of rows is gap 6
in `examples/plan.md`, not a question this rule answers.

## Registration and timing

`schema_verification.yaml` registers column checks in `column_verifications`
and completed-dataset checks in `dataset_verifications`.

Column verifications infer the column on which they are declared. They run
after that column's derivation, conversion, and final override. Dataset
verifications run after every column, output-key validation, and column
verification is complete.

A failed verification fails execution. Implementations must report its stable
specification path, failure count, and representative offending keys. Reporting
limits may be implementation options but must not change pass or fail.

`all_or_none`, `implies`, and `predicate` require an `id`. These IDs must be
unique across the dataset verifications that declare them and should describe
the asserted business rule. Implementations must include the ID in failure
reports in addition to the stable specification path.

## Column verifications

- `not_missing` passes only when every value is non-missing.
- `allowed_values` requires every non-missing value to equal one listed value.
  Missing values pass; combine with `not_missing` when absence is invalid.
- `range` requires every non-missing numeric value to be greater than or equal
  to `min` and less than or equal to `max`, for whichever bounds are supplied.
  At least one bound is required.
- `max_length` requires every non-missing string to be at most `max` Unicode
  code points long. Missing values pass; combine with `not_missing` when
  absence is invalid. It is declared only on a `str` column: a length counts
  the characters of a stored string, and the text a number or a temporal value
  renders as is a property of R011's rendering rather than of the value.
- `matches` requires every non-missing string to match its ECMAScript regular
  expression. Matching searches unless the pattern is anchored.

`max_length` counts code points rather than bytes or UTF-16 units, which is
the unit R006 already fixes for its own `min_length`, so one character is one
count in both R and Python whatever plane it comes from. A length is therefore
a check of its own rather than an anchored `matches` pattern: `.` in an
ECMAScript regular expression counts UTF-16 code units and excludes line
terminators, so the same value could pass in one runtime's spelling of the
bound and fail in another's.

## Dataset verifications

- `unique` requires the listed columns to exist and their combined values to be
  unique. Missing values participate as values; use column `not_missing` when
  they are prohibited.
- `all_or_none` requires at least two distinct columns. For every output row,
  either every listed value must be missing or every listed value must be
  non-missing.
- `implies` evaluates `when` and `then` for every output row. When `when` is
  `TRUE`, `then` must be `TRUE`; a `FALSE` or `UNKNOWN` result from `then`
  fails. When `when` is `FALSE` or `UNKNOWN`, the row passes because the rule
  does not apply.
- `predicate` evaluates `assert` for every output row. Every result must be
  `TRUE`; `FALSE` and `UNKNOWN` fail. It is the escape hatch for row-wise rules
  that do not match a more specific verification type.
- `row_count` requires the output count to meet inclusive `min` and `max`
  bounds. At least one bound is required.

## Errors

- An unknown verification keyword or field: schema failure.
- A duplicate dataset-verification `id`: fail.
- `range` or `row_count` with no bound, or with `min > max`: fail.
- `max_length` whose `max` is less than one: fail. A column that admits no
  value at all is a column the specification should not declare.
- `all_or_none` with fewer than two distinct columns: fail.
- A verification applied to an incompatible column type: fail.
- An unknown column in `unique`, `all_or_none`, `implies`, or `predicate`:
  fail.
- Any verification failure: fail and report it.

The SQL grammar remains draft under R004, so `implies` and `predicate` keep
this rule in draft status.
