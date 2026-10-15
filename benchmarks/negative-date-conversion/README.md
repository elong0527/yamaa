# Reject Date Extraction

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-date-conversion.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one analysis row for each adverse event carrying the
analysis start (`ASTDT`) and a second analysis start (`ASTDT2`).

**Input:** collected adverse event records carrying the reported
start (`AESTDTC`).

**Variables:**

- `ASTDT` would contain the analysis start date taken from the
  reported start.
- `ASTDT2` would be the calendar date taken from the analysis
  start, but taking a calendar date needs a date-time value or date
  text while the analysis start is already a date, so the run is
  rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Use the date directly (`derivation: ASTDT`). When the source is a datetime,
extract its calendar date explicitly:

```yaml
- name: ASTDT2
  type: date
  derivation:
    to_date: {source: ASTDTM}
```
