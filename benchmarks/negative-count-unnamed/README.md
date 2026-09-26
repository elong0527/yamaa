# Reject Unnamed Count

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-count-unnamed.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the analysis date (`ADT`), the numeric result (`AVAL`),
and the baseline flag (`ABLFL`) for each laboratory result, and check that
each subject and parameter has exactly one flagged baseline result.

**Input:** collected laboratory results carrying analysis date
(`ADT`), numeric result (`AVAL`), and baseline flag (`ABLFL`).

**Variables:**

- `ABLFL` is the input baseline flag: `Y` on the record that serves as the
  baseline for the subject and parameter, blank on every other record.

**Note:** the check counts the results flagged `Y` within each subject and
parameter and expects exactly one, but it carries no name, so a report of a
wrong count could not say which study decision the data broke. The run is
rejected before any data is read and no artifact is accepted.

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
