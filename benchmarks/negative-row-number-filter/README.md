# Reject Numeric Filter

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-row-number-filter.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** number collected laboratory results within each subject,
with `VISITSEQ` giving the collection sequence for each result.

**Input:** collected laboratory results carrying the numeric
result (`LBSTRESN`) and the collection sequence.

**Variables:**

- `VISITSEQ` would number each result within its subject in
  collection-sequence order, counting only the results a stated
  condition selects; the other results would keep their rows with
  a missing number. The condition arrives as a bare number (`7`)
  instead of a comparison, so which results it selects is
  undefined, and the run is rejected before any data is read and
  no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Decide which rows the study numbers, then state the rule as a comparison.
When every collected result counts, leave the filter out entirely:

```yaml
- name: VISITSEQ
  type: int
  derivation:
    row_number:
      window:
        group_by: [STUDYID, USUBJID]
        order_by: [LBSEQ]
```

When only some results count, write the condition they satisfy under
`window`, for example `filter: "LBSTRESN > 5.5"`, rather than numbering
the rows by hand. The results it excludes keep their rows with a missing
number.
