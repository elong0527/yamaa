# ADaM ADLB: reject a sequence filtered by a number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-row-number-numeric-filter.html)

This example uses collected laboratory results to record one row per
result:

- `LBSTRESN` is the numeric result of the laboratory test.
- `VISITSEQ` numbers the results in collection order within each
  subject.

The row filter arrives as a number instead of a predicate over the
rows. No implementation may guess which rows a bare number keeps, so
the specification is rejected before any data is read and no artifact
is accepted.

## How to fix

Decide which rows the study numbers, then state the rule as a
predicate. When every collected result counts, leave the filter out
entirely:

```yaml
- name: VISITSEQ
  type: int
  derivation:
    row_number:
      group_by: [STUDYID, USUBJID]
      order_by:
        - {variable: LBSEQ, direction: asc}
```

When only some rows count, write the condition they satisfy rather
than numbering the rows by hand.
