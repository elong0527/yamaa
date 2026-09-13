# Reject a sequence filtered by a number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-row-number-numeric-filter.html)

**Goal:** number collected laboratory results within each subject,
with `VISITSEQ` giving the collection sequence for each result.

**Input:** collected laboratory results carrying the numeric
result (`LBSTRESN`) and the collection sequence.

**Variables:**

- `VISITSEQ` would number each result within its subject in
  collection-sequence order, keeping only rows where the filter
  holds. The filter arrives as a bare number instead of a
  comparison, so no reader may guess which rows it keeps, and the
  run is rejected before any data is read and no artifact is
  accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Decide which rows the study numbers, then state the rule as a comparison.
When every collected result counts, leave the filter out entirely:

```yaml
- name: VISITSEQ
  type: int
  derivation:
    row_number:
      group_by: [STUDYID, USUBJID]
      order_by:
        - {variable: LBSEQ, direction: asc}
```

When only some rows count, write the condition they satisfy rather than
numbering the rows by hand.
