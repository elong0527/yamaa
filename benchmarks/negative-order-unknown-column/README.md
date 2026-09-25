# Reject Unknown Column

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-order-unknown-column.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive the measurement code (`VSTESTCD`) and the numeric
result in standard units (`VSSTRESN`) for each collected
vital-signs measurement, presented by subject and then by visit
number.

**Input:** each input record is one collected vital-signs
measurement carrying the measurement code (`VSTESTCD`), the
numeric result (`VSSTRESN`), and the visit number (`VISITNUM`).

**Variables:**

- `VSTESTCD` would name the measurement (for example `SYSBP`),
  taken from the collected code.
- `VSSTRESN` would hold the numeric result in standard units,
  taken from the collected result.

**Note:** the requested order rests on the visit number, but no
variable of the requested dataset carries it, so a completed record
has no value to be ordered by. The run is rejected before any data
is read and no artifact is accepted.

**Standard:** SDTM | **Domain:** VS

## How to fix

Decide whether the visit number belongs in the dataset. When the order must
rest on it, declare it as a variable and order by that variable; place it in
the artifact columns only when the artifact should carry it:

```yaml
output:
  path: vs.csv
  columns: [STUDYID, USUBJID, VSSEQ, VSTESTCD, VSSTRESN]
  order_by: [USUBJID, VISITNUM]

columns:
  # ... the existing columns, then:
  - name: VISITNUM
    type: float
    label: Visit Number
    derivation: VS_RAW.VISITNUM
```

An unplanned visit takes a sponsor-assigned decimal number (for
example `2.01` after the planned visit it follows), which would fail
`int` conversion, so the declared type is `float`.

When the visit number is not wanted at all, order by a variable the dataset
already declares, such as the sequence number.
