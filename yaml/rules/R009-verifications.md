---
id: R009
title: Verifications
status: draft
applies_to: [root.verifications, column.verifications, column_verifications, dataset_verifications]
depends_on: [R004, R005, R006]
---

# Verifications

## Intent

Define closed, portable assertions over completed output values without using a
generic function or argument bag.

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

## Column verifications

- `not_missing` passes only when every value is non-missing.
- `allowed_values` requires every non-missing value to equal one listed value.
  Missing values pass; combine with `not_missing` when absence is invalid.
- `range` requires every non-missing numeric value to be greater than or equal
  to `min` and less than or equal to `max`, for whichever bounds are supplied.
  At least one bound is required.
- `matches` requires every non-missing string to match its ECMAScript regular
  expression. Matching searches unless the pattern is anchored.

## Dataset verifications

- `unique` requires the listed columns to exist and their combined values to be
  unique. Missing values participate as values; use column `not_missing` when
  they are prohibited.
- `predicate` evaluates its SQL predicate for every output row. Every result
  must be `TRUE`; `FALSE` and `UNKNOWN` fail.
- `row_count` requires the output count to meet inclusive `min` and `max`
  bounds. At least one bound is required.

## Errors

- An unknown verification keyword or field: schema failure.
- `range` or `row_count` with no bound, or with `min > max`: fail.
- A verification applied to an incompatible column type: fail.
- An unknown column in `unique` or `predicate`: fail.
- Any verification failure: fail and report it.

The SQL grammar remains draft under R004, so predicate verification keeps this
rule in draft status.
