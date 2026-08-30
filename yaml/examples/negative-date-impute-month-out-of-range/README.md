# ADaM ADAE: reject a start date completed with no month of the year

This example uses collected adverse events whose start dates are sometimes
recorded only as a year to attempt one record per event:

- `ASTDT` is the start date of the event, completed from the earliest date the
  collected text still allows.

The rule names a fifteenth month. No calendar has one, and carrying the excess
into the following year would make the completed date depend on arithmetic the
rule never described, so the run must fail and no artifact is accepted.

## How to fix

Use a calendar month from 1 through 12. Because this example describes earliest
imputation, January is the consistent correction:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 1
  day: 1
```

A year-only value such as `2023` then becomes `2023-01-01`.
