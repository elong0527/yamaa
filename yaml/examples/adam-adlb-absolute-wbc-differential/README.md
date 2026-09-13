# Calculate absolute lymphocyte counts from white blood cell results

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-absolute-wbc-differential.html)

**Goal:** add absolute lymphocyte (`LYMPH`) records with analysis
value `AVAL` and record-type flag `DTYPE`.

**Input:** collected laboratory records carrying subject identifier
`USUBJID`, visit `VISIT`, parameter code `PARAMCD`, parameter
`PARAM`, and collected result, including white blood cell (WBC)
counts and lymphocyte fractions (`LYMLE`).

**Variables:**

- `AVAL`: on a collected record, the collected result; on a new
  `LYMPH` record, the `WBC` count multiplied by the `LYMLE`
  fraction from the same subject and visit. No new record is added
  when either contributing result is absent or missing, or when a
  `LYMPH` record is already present for the visit.
- `DTYPE`: `CALCULATION` on a new `LYMPH` record; empty on a
  collected record.

**Note:** each contributing parameter may appear at most once
within a subject and visit; a subject and visit with repeated
`WBC` or `LYMLE` results stops the run instead of using a value
picked by amount or position.

**Standard:** ADaM | **Domain:** ADLB
