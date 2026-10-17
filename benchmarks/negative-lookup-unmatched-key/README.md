# Reject Unmatched Key

**Goal:** attach the reference upper limit (`LBSTNRHI`) to each
collected result by test and sex.

**Input:** collected results carrying test code (`LBTESTCD`) and
sex (`SEX`), plus a reference table carrying an upper limit
(`NRHI`) per test code and sex.

**Variables:**

- `LBSTNRHI` would be the upper limit read from the reference
  table matched on test code and sex. A result whose test code and
  sex have no reference entry has no stated answer, so it rejects
  the run with no artifact accepted.

## How to fix

Add the governed `AST/M` range to the reference table when one exists. If an
absent range is intentionally represented by a missing value, declare
`no_match: {literal: null}`:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
    no_match: {literal: null}
```

The lookup then returns a missing value for every column read through it when
a complete key has no match.
