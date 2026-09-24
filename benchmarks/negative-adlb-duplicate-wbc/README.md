# Reject Duplicate WBC

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adlb-duplicate-wbc.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add an absolute lymphocyte (`LYMPH`) record carrying
`AVAL` and `DTYPE`.

**Input:** collected laboratory records carrying the collected
result in `AVAL`, identified by subject, visit, and parameter
code, including white blood cell (WBC) counts and lymphocyte
fractions (`LYMLE`).

**Variables:**

- `AVAL`: the collected result on collected records. On the new `LYMPH`
  record it is the `WBC` count multiplied by the `LYMLE` fraction from the
  same subject and visit; that record is added only when both values are
  present and no `LYMPH` result was collected at the visit.
- `DTYPE`: `CALCULATION` on the new `LYMPH` record, empty on collected
  records.

**Note:** each contributing parameter may occur at most once within a
subject and visit. A repeated `WBC` or `LYMLE` result leaves no single value
to multiply, so the run stops with no artifact rather than choosing one
record by amount or position or adding both values.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Resolve the duplicate WBC records according to the study's data conventions
before calculating the absolute differential. Do not replace `ONLY` with
`MIN`, `MAX`, or file-order selection unless that choice is a documented
clinical rule; those alternatives answer a different question.
