# Reject Multiple Baselines

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-multiple-baselines.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the numeric result (`LBSTRESN`) and repeat the
baseline value (`BAVL`) on every row for the subject.

**Input:** collected lab (LB) records, each with a numeric result
(`LBSTRESN`) and a baseline flag (`LBBLFL`) holding `Y` on the
baseline record.

**Variables:**

- `LBSTRESN`: the numeric result copied from the input record.
- `LBBLFL`: the baseline flag copied from the input record, which
  `BAVL` reads to find the subject's baseline row.
- `BAVL`: the baseline value, taken from the subject's row
  flagged `Y` in `LBBLFL` and repeated on every row for that
  subject.

**Note:** the run stops because more than one record carries the
flag, so no single starting point exists and no row is produced.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Keep exactly one baseline record per subject. Either correct the flag on the
visit that is not the baseline:

```yaml
LBBLFL: " "
```

or, when both visits genuinely qualify, pick one by rule (for example the
earliest) and flag only that row before deriving.
