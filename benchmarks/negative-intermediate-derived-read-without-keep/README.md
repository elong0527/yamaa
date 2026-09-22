# Reject a Derived Read Without keep

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-intermediate-derived-read-without-keep.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attach the uppercased treatment name (`TRT_U`) of the last
non-ongoing exposure record to each subject.

**Input:** exposure records with sequence numbers (`EXSEQ`), treatment
names (`EXTRT`), and ongoing flags (`EXONGO`) in mixed case.

**Variables:**

- `AVALC` would contain the uppercased treatment name from the
  intermediate's selected record.

**Note:** the intermediate derives `TRT_U` but declares no `keep`, so the
read is not row-scoped: without a single selected record there is no one
computed value to read. The planner rejects the derived `ID.name` read as
`unknown_field` before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

Declare `keep` with an `order_by` so the intermediate selects exactly one
record per subject, which makes the derived value a row-scoped read:

```yaml
intermediates:
  - id: LASTEX
    dataset: EX
    order_by:
      - variable: EX.TRT_U
        direction: desc
    keep: first
```
