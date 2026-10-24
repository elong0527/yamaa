# Reject fixed-decimal text made from a text result

**Goal:** build `AVALC`, each laboratory result as text with two decimals,
right-aligned in eight characters.

**Input:** laboratory (LB) records carrying each result as collected text
(`LBSTRESC`) and, where it is a number, as a number (`LBSTRESN`).

**Variables:**

- `AVALC` would be the collected text result written with two decimals.

Text has no value to round, so the run is rejected before any data is read
and no artifact is accepted.

## How to fix

Format the numeric result the collected text was made from, and declare its
type: a field read without a declared type is text, and is rejected the same
way. A result collected only as text, such as `<0.5`, then has no number and
takes the `missing` answer.

```yaml
input:
  LB: {path: input/lb.csv, types: {LBSTRESN: float}}

# in the AVALC column
str_decimal: {source: LB.LBSTRESN, decimals: 2, width: 8, missing: "."}
```
