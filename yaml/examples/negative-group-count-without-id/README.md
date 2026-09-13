# Reject a baseline count check without a name

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-group-count-without-id.html)

**Goal:** carry the analysis date (`ADT`), the numeric result
(`AVAL`), and the baseline flag (`ABLFL`, `Y` on the baseline
result and blank otherwise) for each subject and parameter, and
check that each subject and parameter has exactly one flagged
baseline result. The count check carries no name, so a report of
a wrong count could not say which study decision the data broke.
The run is rejected before any data is read and no artifact is
accepted.

**Input:** collected laboratory results carrying analysis date
(`ADT`), numeric result (`AVAL`), and baseline flag (`ABLFL`).

**Variables:**

- `ADT` would be the analysis date, carried over from the
  collected records.
- `AVAL` would be the numeric result, carried over from the
  collected records.
- `ABLFL` would be `Y` on the baseline result and blank
  otherwise; the check counts rows where `ABLFL` equals `Y`
  within each subject and parameter and expects exactly one.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Name the rule the count asserts, in the words a reviewer would use for it:

```yaml
verifications:
  - row_count:
      id: one-baseline-record-per-subject-and-parameter
      group_by: [STUDYID, USUBJID, PARAMCD]
      filter: "ABLFL = 'Y'"
      min: 1
      max: 1
```

A count over the whole artifact rather than within a group asserts its size
instead of a study rule and needs no name, so `min` and `max` alone remain
valid there.
