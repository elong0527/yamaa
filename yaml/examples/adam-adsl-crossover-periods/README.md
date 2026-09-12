# ADaM ADSL: derive period-scoped treatments and dates across a washout

This example uses sample DM and EX data and a `yamaa` specification to derive
one row per subject in a two-period crossover:

- `TR01SDT` and `TR01EDT` are the first start date and the last end date of the
  subject's exposure in period one; `TR02SDT` and `TR02EDT` are the same two
  dates for period two;
- `TRT01A` and `TRT02A` are the treatments given in each period, each taken
  from the subject's earliest exposure record within that period; when two
  records start on the same day the lower sequence number wins;
- `WASHDUR` is the length of the washout, the number of days strictly between
  `TR01EDT` and `TR02SDT`, counting neither the last day of period one nor the
  first day of period two, so consecutive periods give zero.

Every value is read only from the exposure records belonging to its own period,
so one period's dates and treatment never mix with the other's. A subject who
never entered period two has no exposure records there, and every period-two
variable is empty for them.
