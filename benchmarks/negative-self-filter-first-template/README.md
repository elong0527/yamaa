# Reject Missed Visits Chosen Before Any Visit Is Recorded

**Goal:** list each subject's planned visits, with the visit number in
`VISITNUM`, the visit name in `VISIT`, and whether the visit took place in
`SVOCCUR`: one record per visit that took place, and one per planned visit
that has none.

**Input:** each subject's planned visits, and the visits that took place,
each carrying the visit number (`VISITNUM`) and name (`VISIT`).

**Variables:**

- `SVOCCUR` would be `Y` for a visit that took place and `N` for a planned
  visit with no visit that took place.

**Note:** a planned visit is listed as missed only when no record of that
visit has been built before it. The missed visits are built first, before
any visit that took place has a record, so every planned visit would look
missed. The run is rejected before any data is read and no artifact is
accepted.

## How to fix

Build the records of the visits that took place first, then the missed
visits, so a planned visit is compared with the visits already recorded:

```yaml
rows:
  - id: attended
    dataset: VISITS
    derivations: ...
  - id: missed
    dataset: PLAN
    filter: "RECORDED.VISIT IS NULL"
    derivations: ...
```
