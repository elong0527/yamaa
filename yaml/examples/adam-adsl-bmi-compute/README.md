# ADaM ADSL: compute BMI from height and weight

This example uses sample DM data and a `yamaa` specification to derive one row
per subject:

- `HEIGHTCM` and `WEIGHTKG` are the collected height and weight;
- `BMI` is body mass index, weight divided by the square of height in metres.
  A subject with no height has no BMI, and so does one recorded with a height
  of zero: dividing by it is stopped deliberately rather than left to produce
  an error or a silent value.
