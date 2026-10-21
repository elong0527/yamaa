# Reject a Lookup That Names No Match Columns

**Goal:** attach the reference-range upper limit (`LBSTNRHI`) to
each collected result, carrying its test code and sex.

**Input:** collected results carrying test code (`LBTESTCD`), sex
(`SEX`), and numeric result (`LBSTRESN`), plus a reference-limit
table carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would contain the upper limit value from the
  reference-limit table for the test code and sex of the result.

**Note:** the limit-table columns to match on are not stated, and
they are never guessed from the output identifiers or from the
columns the two tables share, so the run is rejected before any data
is read and no artifact is accepted.

## How to fix

Name the lookup-table columns to match on:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

Each listed column matches the same-named current-row value. When the
current-row name differs, map the column to it, as in
`key: {TESTCD: LBTESTCD, SEX: SEX}`. A lookup always states its match: it
never falls back to the output keys or to the input's declared `key`.
