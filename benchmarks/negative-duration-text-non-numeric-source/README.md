# Reject a duration written from minutes held as text

**Goal:** write each administration's collected duration as ISO 8601
duration text (`EXDUR`).

**Input:** collected exposure records, one per administration (`EXSEQ`),
with the duration in whole minutes as the case report form collected it.

**Variables:**

- `EXDUR` would hold the duration as ISO 8601 duration text, so 90 minutes
  gives `PT1H30M`. Only a number of minutes is written as a duration, and
  the collected minutes are still text that nothing has read as a number,
  so the run is rejected before any data is read and no artifact is
  accepted.

## How to fix

Declare the collected minutes a whole number where the input is read, so
each value is checked as a number as the records are read:

```yaml
input:
  EX_RAW: {path: input/ex_raw.csv, types: {EXSEQ: int, EXDURMIN: int}}
```

Declare `float` instead when the form collects fractions of a minute; the
fraction is written as seconds, so 1.5 minutes gives `PT1M30S`.
