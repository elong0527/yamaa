# Ordered Float Sum

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-ordered-sum.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add one total record per subject and visit, computed by adding
the collected component results in the order the source records were
stored.

**Input:** collected laboratory (LB) records per subject (`USUBJID`) and
visit (`VISIT`), each carrying a test code (`LBTESTCD`), test name
(`LBTEST`), and standardized numeric result (`LBSTRESN`).

**Variables:**

- `PARAM`: the collected label on each component record, and
  "Total of Components" on the new total record.
- `AVAL`: the collected result on each component record; on the total
  record, the component results added in their stored record order. A
  missing result is skipped, and the total is empty when every result is
  missing.
- `DTYPE`: empty on the collected records; `CALCULATION` on the total
  record, marking the value as calculated rather than collected.

**Note:** binary floating-point addition makes the total sensitive to that
order: adding `0.1`, `0.2`, and `0.3` gives `0.6000000000000001`, while
adding the same values in reverse gives `0.6`.

**Standard:** ADaM | **Domain:** ADLB
