# Reject a Fractional Count of Administrations

**Goal:** give one row for each administration a collected exposure (EX)
record counts, carrying the collected record's `EXSEQ`, `EXTRT`, `EXDOSE`
and `EXDOSU` through, numbering each administration in `ADOSEN` beside the
record's count in `NDOSE`.

**Input:** one `spec.yaml` over collected exposure records, each with its
treatment, dose, unit and the number of administrations it stands for
(`EXDOSCNT`), read as a number that may hold a fraction.

**Variables:**

- `ADOSEN` numbers the administrations built from one collected record from
  `1` up to its count.
- `NDOSE` is how many administrations the collected record stands for,
  taken from `EXDOSCNT`.

**Note:** a count is a whole number, so a count read as a number that may
hold a fraction is refused before any record is read, and no artifact is
accepted.

## How to fix

Read the count as a whole number:

```yaml
input:
  EX:
    path: input/ex.csv
    types: {EXSEQ: int, EXDOSE: float, EXDOSCNT: int}
```

When the collected value really holds a fraction, such as half a tablet
recorded as `0.5`, it is an amount and not a count of administrations:
count from the field that says how many times the dose was given.
