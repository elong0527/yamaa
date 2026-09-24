# Reject Multiple Baselines

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-multiple-baselines.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the numeric result (`LBSTRESN`) and the baseline
flag (`LBBLFL`) as collected, and repeat the baseline value (`BAVL`)
on every row for the subject.

**Input:** collected lab (LB) records, each with a numeric result
(`LBSTRESN`) and a baseline flag (`LBBLFL`) holding `Y` on the
baseline record.

**Variables:**

- `BAVL`: the baseline value, taken from the subject's row
  flagged `Y` in `LBBLFL` and repeated on every row for that
  subject; missing when the subject has no flagged row.

**Note:** the run is rejected when a subject has more than one
record flagged `Y`: no single baseline value exists, so no artifact
is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Keep exactly one baseline record per subject. Either clear the flag in the
collected data on the visit that is not the baseline, for example `WEEK 1`:

```csv
STUDYID,USUBJID,VISIT,LBSTRESN,LBBLFL
PILOT7,P7-901,SCREENING,5.1,Y
PILOT7,P7-901,WEEK 1,5.4,
PILOT7,P7-901,WEEK 2,5.2,
```

or, when both visits genuinely qualify, pick one by rule (for example the
earliest) and flag only that row before deriving.
