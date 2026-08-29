# ADaM ADSL: select actual treatment and its duration from EX

This example uses sample DM and EX data and a `yamaa` specification to derive
one row per subject:

- `TRT01A` is the treatment the subject actually received, taken from their
  earliest exposure record; when two records start on the same day the lower
  sequence number wins. A subject with no exposure takes their planned arm from
  DM instead, and a subject with neither is `NOT TREATED`. The value is
  uppercased;
- `TRTSDT` is the subject's first exposure start date and `TRTEDT` is their
  last exposure end date;
- `TRTDURD` is the number of days from `TRTSDT` to `TRTEDT`, counting both the
  first and the last day, so a subject treated on a single day has a duration
  of one;
- `SAFFL` is `Y` for a subject who has a treatment start date and `N`
  otherwise.
