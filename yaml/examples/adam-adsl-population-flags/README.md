# ADaM ADSL: derive the safety and intent-to-treat flags

This example uses a pre-derived ADSL slice and a `yamaa` specification to
derive one row per subject:

- `ARMCD` and `TRTSDT` are carried through as given: the subject's planned
  treatment arm and the date they started treatment;
- `SAFFL` is `Y` for a subject who has a treatment start date and `N`
  otherwise, so it marks the subjects who received any treatment;
- `ITTFL` is `Y` for a subject who has a planned arm and `N` otherwise, so it
  marks the subjects who were randomized.

The two flags are independent: a subject can be randomized without being
treated, and each flag names the one fact that justifies it. Taking the arm and
the treatment start date as given keeps the flag rules visible on their own.
