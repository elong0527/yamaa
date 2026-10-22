# Reject a Baseline Check That Matches Subjects Through Another Dataset

**Goal:** carry each laboratory result in `AVAL` and check that every result
has a baseline value for its subject and test.

**Input:** laboratory results carrying the test code (`LBTESTCD`) and the
numeric result (`LBSTRESN`), demographics records, and a table of baseline
values (`BASE`) by subject and test code.

**Variables:**

- `AVAL` would be the numeric result.

**Note:** the check looks up each result's baseline value. It matches the
subject through the demographics records, but the results, not the
demographics records, build the rows, so a result has no demographics record
of its own to match. The run is rejected before any data is read and no
artifact is accepted.

## How to fix

Match the subject of the result itself, the row's own `USUBJID`:

```yaml
intermediates:
  - id: BASELINE
    dataset: LBBASE
    key: [USUBJID, LBTESTCD]
```
