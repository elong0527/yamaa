# Reject Mismatched Lists

**Goal:** attach the reference-range upper limit (`LBSTNRHI`) to
each collected result, carrying its test code and sex.

**Input:** collected results carrying test code (`LBTESTCD`), sex
(`SEX`), and numeric result (`LBSTRESN`), plus a reference-limit
table carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would contain the upper limit value from the
  reference-limit table for the test code and sex of the result.

**Note:** the lookup names three limit-table columns, and each listed
column matches the same-named current-row value. The collected results
carry no unit, so the `UNIT` column would match against nothing. The
unknown current-row value is rejected before any data is read; no artifact
is accepted.

## How to fix

If the limit table holds one unit per test code, match on test code and sex
alone, since the collected results carry no unit:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

When a current-row name differs from the table's, map the table column to
it, as in `key: {LBTESTCD: PARAMCD, SEX: SEX}`.
