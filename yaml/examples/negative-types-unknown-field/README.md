# ADaM ADEX: reject a total over a field the source does not have

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-types-unknown-field.html)

This example uses a subject-treatment inventory with its component exposure
records to attempt one record per subject and treatment:

- `DOSECUM` is the total dose administered across the exposure records.

The exposure records are described as carrying a numeric field they do not
have, and the field they do carry is left as text. Ignoring the description
would total text, and applying it to the similarly named field would guess
which one was meant, so the run must fail and no artifact is accepted.

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

The qualified aggregate can then resolve `EX.EXDOSE` as numeric.
