# Derive mean arterial pressure records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-map.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add one mean arterial pressure (`MAP`) record, holding the result
in `AVAL` and `CALCULATION` in `DTYPE`, for each subject and visit that has
both a systolic (`SYSBP`) and a diastolic (`DIABP`) blood pressure result;
every collected record stays in the data.

**Input:** vital-signs records carrying a collected result (`AVAL`) for
systolic (`SYSBP`) and diastolic (`DIABP`) blood pressure, one record per
subject, visit, and parameter.

**Variables:**

- `AVAL`: the collected result, or two-thirds of the diastolic pressure plus
  one-third of the systolic pressure on the added record. No record is added
  when either contributor is absent from the visit or its result is missing.
- `DTYPE`: `CALCULATION` on the added record and blank on collected records.

**Note:** a subject and visit with more than one `SYSBP` or `DIABP` record
stops the run rather than choosing one, and so does an input that already
carries a `MAP` record.

**Standard:** ADaM | **Domain:** ADVS
