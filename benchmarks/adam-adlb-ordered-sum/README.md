# Ordered Float Sum

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-ordered-sum.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add one total laboratory record per subject and visit whose
analysis value (`AVAL`) is the collected component results added in the
order the source records were stored, flagged with the record-type flag
(`DTYPE`).

**Input:** collected laboratory records carrying the study identifier,
subject identifier, sequence number, test code and name, visit, and
standardized result. The component values are deliberately simple
(`0.1`, `0.2`, `0.3`) so the floating-point rounding stays visible.

**Variables:**

- `PARAM`: the collected test name on each component record;
  "Total of Components" on the new total record.
- `AVAL`: the collected result on each component record; on the total
  record, the component results added in stored record order. A component
  with no collected result contributes nothing, and a subject and visit
  with no collected results at all leaves the total empty rather than
  zero.
- `DTYPE`: empty on the collected records; `CALCULATION` on the total
  record, marking it as calculated rather than collected.

**Note:** binary floating-point addition makes the total sensitive to
that order: one subject's records stored as `0.1`, `0.2`, `0.3` total
`0.6000000000000001`, while another subject's identical values stored as
`0.3`, `0.2`, `0.1` total `0.6`.

**Standard:** ADaM | **Domain:** ADLB
