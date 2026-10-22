# Reject Unmatched Key

**Goal:** attach the reference upper limit (`LBSTNRHI`) to each
collected result by test and sex.

**Input:** collected results carrying test code (`LBTESTCD`) and
sex (`SEX`), plus a reference table carrying an upper limit
(`NRHI`) per test code and sex.

**Variables:**

- `LBSTNRHI` would be the upper limit read from the reference
  table matched on test code and sex. Every result is required to
  find its reference entry, so a result whose test code and sex
  have none rejects the run with no artifact accepted.

## How to fix

Add the governed `AST/M` range to the reference table when one exists. If an
absent range is intentionally represented by a missing value, drop
`require_match: true`:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

The lookup then returns a missing value for every column read through it when
a complete key has no match, and the run counts each such answer under
`absent`. Declaring `absent` beside `require_match: true` is refused as a
contradiction.
