# Reject Incomparable Endpoints

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-bad-range.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attach the epoch (`EPOCH`) containing each collected
vital-sign date.

**Input:** vital-sign records carrying a collected calendar date
(`VSDTC`), plus an epoch table carrying a period name (`EPOCH`)
with integer day bounds (`DYLO`, `DYHI`).

**Variables:**

- `EPOCH` would be the period name from the epoch-table row whose
  day range contains the visit, and blank when no range does. A
  calendar date cannot be ordered directly against integer day
  bounds, so the run is rejected before any data is read and no
  artifact is accepted.

**Standard:** SDTM | **Domain:** VS

## How to fix

Prefer comparing the collected date with date bounds directly: match
`VSDTC` between the subject's own element start and end dates
(`SESTDTC`, `SEENDTC` from the subject-elements data), which is how
SDTM assigns `EPOCH`:

```yaml
between: {value: VSDTC, lower: SESTDTC, upper: SEENDTC}
```

Only when the epoch table must stay study-day based, derive the
integer study day from the date and the subject's reference start
date (`RFSTDTC`, read from demographics into a date column), then
compare that value with the integer bounds:

```yaml
- name: VSDY
  type: int
  derivation:
    study_day:
      date: VSDTC
      reference: RFSTDTC
```

```yaml
between: {value: VSDY, lower: DYLO, upper: DYHI}
```

The study-day route needs demographics as an extra input, and it gives
every subject the same epoch boundaries.

[`sdtm-vs-study-day`](../sdtm-vs-study-day/) attaches the epoch this way.
