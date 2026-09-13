# Reject a previous weight that names no earlier visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-row-value-zero-offset.html)

**Goal:** derive `ADT`, `AVAL`, and `PREVAL` for collected weight
measurements, where `PREVAL` holds the weight from the prior
visit.

**Input:** collected weight records carrying a collection date
(`VSDTC`) and a numeric weight (`VSSTRESN`).

**Variables:**

- `ADT` would be the analysis date, carried over from the
  collected date.
- `AVAL` would be the analysis weight, carried over from the
  collected numeric weight.
- `PREVAL` would be the previous analysis weight, taken from the
  prior visit in subject visit order.

The request names a zero offset along the visit order, so it asks
for the visit itself. A record's own weight is already the
analysis weight, and a second name for it would let two spellings
of one value drift apart, so the run is rejected before any data
is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Move to the preceding row with a negative step in the stated ascending visit
order:

```yaml
row_value:
  source: AVAL
  offset: -1
  group_by: [STUDYID, USUBJID]
  order_by: [ADT, VSSEQ]
```

The first row for each subject then stays missing because it has no preceding
visit. If the current value is intended, use the analysis weight directly
instead of a neighboring-row read.
