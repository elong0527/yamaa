# Reject a zero text width

**Goal:** write each analysis value (`AVAL`) as text with two decimals,
right-aligned, in `AVALC`.

**Input:** laboratory (LB) records carrying a numeric analysis value.

**Variables:**

- `AVALC` would be the analysis value with two decimals, right-aligned in
  the stated number of characters. That number is zero, and text fills at
  least one character, so the run is rejected before any data is read.

## How to fix

Set the width to the number of characters every value must fill, or leave it
out to write each value without leading spaces:

```yaml
str_decimal: {source: AVAL, decimals: 2, width: 8}
```
