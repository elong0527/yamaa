# ADaM ADAE: reject a start date completed with no month of the year

This example uses collected adverse events whose start dates are sometimes
recorded only as a year to attempt one record per event:

- `ASTDT` is the start date of the event, completed from the earliest date the
  collected text still allows, and only where the collected text already
  carries a month. A start date recorded as a year alone is left without an
  analysis date rather than given both a month and a day.

The rule names a fifteenth month. No calendar has one, and carrying the excess
into the following year would make the completed date depend on arithmetic the
rule never described, so the run must fail and no artifact is accepted.

The fifteenth month is rejected even though no record here can reach it. Of the
two events, one carries a year alone and is therefore left without an analysis
date, and the other carries its own month and day and is used as collected, so
the declared month would never be read. A limit on how much may be supplied
narrows which records are completed; it does not excuse a value no calendar
has.

## How to fix

Use a calendar month from 1 through 12. Because this example describes earliest
imputation, January is the consistent correction:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 1
  day: 1
  minimum_source_precision: month
```

A year-and-month value such as `2023-06` then becomes `2023-06-01`. A year-only
value such as `2023` is still left without an analysis date, because the
declared minimum forbids supplying both a month and a day. Dropping
`minimum_source_precision` as well would let `2023` become `2023-01-01`.
