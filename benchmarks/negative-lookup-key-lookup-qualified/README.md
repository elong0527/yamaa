# Reject a Reference Range Matched on a Looked-Up Sex

**Goal:** attach the reference-range limits (`LBSTNRLO`, `LBSTNRHI`) to each
collected result, by its test code (`LBTESTCD`) and the subject's sex.

**Input:** collected results carrying the test code and the numeric result
(`LBSTRESN`), the subjects' demographics carrying their sex, and a
reference-limit table by test code and sex.

**Variables:**

- `LBSTNRLO` and `LBSTNRHI` would be the lower and upper limits of the
  reference range for the result's test and the subject's sex.

**Note:** a reference range is matched on values the result row carries.
The sex is taken from another look-up while the range is matched, and a
match never reads one look-up through another, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Bring the subject's sex onto the row as a column first, then match the
range on that column:

```yaml
columns:
  - {name: SEX, type: str, derivation: SUBJECT.SEX}
intermediates:
  - id: RANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

A column that only carries the value for the match can stay out of the
output.
