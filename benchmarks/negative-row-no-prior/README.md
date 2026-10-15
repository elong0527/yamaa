# Reject Missing Prior

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
  subject's prior record in date order, with ties broken by
  sequence number.

The request steps zero records along that order, so it asks for
the record itself. A record's own weight is already the
analysis weight, and a second name for it would let two spellings
of one value drift apart, so the run is rejected before any data
is read and no artifact is accepted.

## How to fix

Move to the preceding row with a negative step in the stated ascending visit
order:

```yaml
row_value:
  source: AVAL
  offset: -1
  window:
    group_by: [STUDYID, USUBJID]
    order_by: [ADT, VSSEQ]
```

The first row for each subject then stays missing because it has no preceding
visit. If the current value is intended, use the analysis weight directly
instead of a neighboring-row read.
