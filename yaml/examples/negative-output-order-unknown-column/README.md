# Reject ordering by a visit number the dataset does not carry

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-output-order-unknown-column.html)

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

The requested order rests on the visit number, but no variable of
the requested dataset carries it, so no completed record has a
value to be ordered by. Ordering by something the records do not
contain has no meaning the run could give it, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** SDTM | **Domain:** VS

## How to fix

Decide whether the visit number belongs in the dataset. When the order must
rest on it, declare it as a variable and order by that variable; place it in
the artifact columns only when the artifact should carry it:

```yaml
output:
  columns: [STUDYID, USUBJID, VSSEQ, VSTESTCD, VSSTRESN]
  order_by: [USUBJID, VISITNUM]

columns:
  - name: VISITNUM
    type: int
    label: Visit Number
    derivation:
      source: VS_RAW.VISITNUM
```

When the visit number is not wanted at all, order by a variable the dataset
already declares, such as the sequence number.
