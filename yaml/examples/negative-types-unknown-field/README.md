# Reject a total over an unknown exposure field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-types-unknown-field.html)

**Goal:** attempt `EXTRT` and the total dose (`DOSECUM`) for each
subject and treatment, totalling the dose across exposure records.

**Input:** subject-treatment records carrying treatment (`EXTRT`),
and exposure records carrying treatment (`EXTRT`), sequence number
(`EXSEQ`), and dose (`EXDOSE`).

**Variables:**

- `EXTRT` would be the treatment name, carried over from the
  subject-treatment records.
- `DOSECUM` would be the total dose across the exposure records,
  but the field list also names a field the exposure records do not
  carry. Guessing which field was meant would total the wrong
  values, so the run is rejected before any data is read and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

Correct the field name in the dataset type declaration so it names the column
that actually exists:

```yaml
datasets:
  TRT: input/subject_treatment.csv
  EX:
    path: input/ex.csv
    types:
      EXDOSE: float
```

The qualified total can then resolve the dose as numeric.
