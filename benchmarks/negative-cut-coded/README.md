# Reject Coded Age

**Goal:** build `AGEGRP` to place each subject into an age band,
with `younger` below 65 and `elderly` at or above 65.

**Input:** collected demographics carrying numeric age (`AGE`)
and coded sex (`SEX`).

**Variables:**

- `AGEGRP` would be `younger` below 65 and `elderly` at or above 65.

**Note:** the bands are applied to the coded sex (`SEX`) instead of the age.
Placing a coded value into numeric bands has no meaning, so the run is
rejected before any data is read and no artifact is accepted.

## How to fix

Band the numeric age rather than the coded sex. The demographics file is
read as text unless a field is given a type, so declare `AGE` as a whole
number on the input:

```yaml
input:
  DM: {path: input/dm.csv, types: {AGE: int}}
```

and band that age:

```yaml
cut:
  source: DM.AGE
  breaks: [65]
  labels: [younger, elderly]
```
