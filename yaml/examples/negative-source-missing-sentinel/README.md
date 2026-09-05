# ADaM ADSL: reject an age recorded as NA

This example uses a collected subject listing to attempt one record per
subject:

- `AGE` is the age in years at screening.

One age is stored as the letters `NA`. Those letters are not a number, and
reading them as an age nobody collected would decide, on the study's behalf,
that this is how it records absence. A study that records absence with a code
says so where a reader can see it, so the run must fail and no artifact is
accepted.

## How to fix

Leave the field empty when an age was not collected, and keep reading the
field as a number:

```yaml
datasets:
  DM:
    path: input/dm.csv
    types:
      AGE: int
```

If the code carries a meaning worth keeping, such as an age withheld rather
than never taken, read the field as text and map the code to a value the
study defines.
