# Reject rounding a coded value

**Goal:** build `RNDVAL` by rounding a value to one decimal place
with ties half away from zero.

**Input:** collected demographics carrying coded sex (`SEX`).

**Variables:**

- `RNDVAL` would be the coded sex (`SEX`) rounded to one decimal
  place.

Rounding a coded value has no meaning, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Round a numeric variable rather than the coded sex. The demographics
input carries none here, so add one and declare its type: a field read
without a declared type is text, and rounding it is rejected the same
way.

```yaml
input:
  DM: {path: input/dm.csv, types: {AVAL: float}}

# in the RNDVAL column
round_half_away_from_zero:
  source: DM.AVAL
  digits: 1
```
