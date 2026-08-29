# ADaM ADAE: impute partial dates

This example uses sample AE and ADSL data and a `yamaa` specification to:

- complete `AESTDTC` into the analysis date `ASTDT` with `date_impute`, using
  month `6` and day `15` for the components the collected value does not carry.
  `UNKNOWN` is an invalid date and an uncollected value is a missing one; both
  yield no analysis date;
- render `ASTDT` back as the character date `ASTDTC`, which R011 writes as
  ISO 8601;
- flag an event starting on or after `ADSL.TRTSDT` as `TRTEMFL`. An imputed day
  decides the flag by the same rule as a collected one.