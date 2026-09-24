# Derive absolute lymphocyte records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-lymphocytes.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add absolute lymphocyte (`LYMPH`) records whose
analysis value (`AVAL`) is the white blood cell count times
the lymphocyte fraction, and flag them with the record-type
flag (`DTYPE`).

**Input:** collected laboratory records carrying the subject
identifier (`USUBJID`), visit (`VISIT`), parameter code
(`PARAMCD`), parameter name (`PARAM`), and collected result
(`AVAL`): white blood cell counts (`WBC`), lymphocyte
fractions (`LYMLE`), and unrelated parameters.

**Variables:**

- `AVAL`: the collected result on a collected record; on an
  added `LYMPH` record, the `WBC` count times the `LYMLE`
  fraction for the same subject and visit.
- `DTYPE`: `CALCULATION` on an added `LYMPH` record; empty on
  a collected record.

**Note:** a new `LYMPH` record is added only when both a
`WBC` count and a `LYMLE` fraction are present for the
subject and visit, and no `LYMPH` record is already there.
Each contributing parameter may appear at most once within
a subject and visit; a subject and visit with repeated
`WBC` or `LYMLE` results stops the run instead of using a
value picked by amount or position.

**Standard:** ADaM | **Domain:** ADLB
