# Mean Arterial Pressure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-map.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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

**Note:** a subject and visit with more than one `SYSBP` or
`DIABP` record stops the run rather than choosing one, and so does
a `MAP` record already present in the input.

**Standard:** ADaM | **Domain:** ADVS
