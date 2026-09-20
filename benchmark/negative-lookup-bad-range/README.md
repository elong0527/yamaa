# Reject Incomparable Endpoints

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-bad-range.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** attach the epoch (`EPOCH`) containing each collected
vital-sign date.

**Input:** vital-sign records carrying a collected calendar date
(`VSDTC`), plus an epoch table carrying a period name (`EPOCH`)
with integer day bounds (`DYLO`, `DYHI`).

**Variables:**

- `EPOCH` would be the period name from the epoch-table row whose
  day range contains the visit.

A calendar date cannot be ordered directly against integer day
bounds, so the run is rejected before any data is read and no
artifact is accepted.

**Standard:** SDTM | **Domain:** VS

## How to fix

Derive the integer study day from the date and the subject's reference date,
then compare that value with the integer bounds:

```yaml
between: {value: VSDY, lower: DYLO, upper: DYHI}
```
