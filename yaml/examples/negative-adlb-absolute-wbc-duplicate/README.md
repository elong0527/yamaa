# Reject duplicate white blood cell inputs for an absolute differential

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adlb-absolute-wbc-duplicate.html)

**Goal:** add an absolute lymphocyte (`LYMPH`) record carrying
`AVAL` and `DTYPE`.

**Input:** collected laboratory records carrying the collected
result in `AVAL`, identified by subject, visit, and parameter
code, including white blood cell (WBC) counts and lymphocyte
fractions (`LYMLE`).

**Variables:**

- `AVAL`: would contain the `WBC` count multiplied by the
  `LYMLE` fraction from the same subject and visit on a new
  `LYMPH` record, but no row is produced when more than one
  `WBC` record shares a subject and visit, since no single value
  is available to use. The run fails rather than choosing one
  record or adding both values.
- `DTYPE`: would be `CALCULATION` on the new `LYMPH` record, but
  no row is produced while the contributing values remain
  ambiguous.

The run is rejected while the new record is being assembled, and
no artifact is accepted.

**Note:** each contributing parameter may occur at most once
within a subject and visit; a repeated result stops the run
instead of being resolved by amount or position.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Resolve the duplicate WBC records according to the study's data conventions
before calculating the absolute differential. Do not replace `ONLY` with
`MIN`, `MAX`, or file-order selection unless that choice is a documented
clinical rule; those alternatives answer a different question.
