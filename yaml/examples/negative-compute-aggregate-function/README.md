# Reject a subject total written as a row formula

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-compute-aggregate-function.html)

**Goal:** attempt the subject-level total result (`AVALTOT`)
from the collected results for a subject, written as the row
formula `SUM(AVAL)`.

**Input:** laboratory (LB) records carrying the study and
subject identifiers, the test code (`LBTESTCD`) used for the
parameter code, and the collected numeric result (`LBSTRESN`).

**Variables:**

- `AVAL`: the collected numeric result copied from `LBSTRESN`;
  missing when the collected result is missing.
- `AVALTOT`: would be the total of `AVAL` across the subject's
  records, repeated on each record for that subject, but no value
  is produced.

The expression `SUM(AVAL)` is written as a row formula, but a row
formula sees one record at a time and cannot reach the other
records it would have to total, so the run is rejected before any
data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Use an aggregate and state the subject-level grain explicitly:

```yaml
derivation:
  aggregate:
    group_by: [STUDYID, USUBJID]
    expr: "SUM(AVAL)"
```

The total is then broadcast to each parameter row for that subject.
