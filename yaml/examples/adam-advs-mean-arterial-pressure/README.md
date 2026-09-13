# Add a mean arterial pressure record per visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-mean-arterial-pressure.html)

**Goal:** add a mean arterial pressure (`MAP`) record, holding
its result in `AVAL` and `CALCULATION` in `DTYPE`, at each
subject and visit with both a systolic blood pressure (`SYSBP`)
and a diastolic blood pressure (`DIABP`) result, keeping each
collected record.

**Input:** vital-signs records carrying a collected result
(`AVAL`) for systolic (`SYSBP`) and diastolic (`DIABP`) blood
pressure at each subject and visit.

**Variables:**

- `AVAL`: the collected result, or two-thirds of the diastolic
  pressure plus one-third of the systolic pressure on the added
  record; no record is added when either contributor is absent
  or missing.
- `DTYPE`: `CALCULATION` on the added record and blank on
  collected records.

**Note:** each subject and visit carries at most one `SYSBP`
record and one `DIABP` record, and carries no `MAP` record.

**Standard:** ADaM | **Domain:** ADVS
