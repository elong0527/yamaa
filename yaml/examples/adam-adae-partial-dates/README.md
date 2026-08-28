# ADaM ADAE: impute partial dates and flag the imputation

This example uses sample AE and ADSL data and a `yamaa` specification to:

- extract the year, month, and day of `AESTDTC` as `YR0`, `MO0`, and `DA0`,
  which stay internal to the derivation;
- default a missing month or day to `01` as `MOI` and `DAI`;
- rebuild an ISO date as `ASTDTC` and convert it to a date as `ASTDT`. A value
  with no four-digit year, such as `UNKNOWN`, and an uncollected value both
  yield no analysis date;
- record the imputed component as `ASTDTF`, using `M` when the month was
  imputed and `D` when only the day was imputed, and verify its allowed values;
- flag an event starting on or after `ADSL.TRTSDT` as `TRTEMFL`. An imputed day
  decides the flag by the same rule as a collected one.
