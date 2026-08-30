# ADaM ADAE: reject a start date completed from text that is not a date

This example uses collected adverse events whose start dates are sometimes
incomplete to attempt one record per event:

- `ASTDT` is the start date of the event, completed from the earliest date the
  collected text still allows.

One start date was entered as a word rather than as a date or the beginning of
one. A date that was never collected and text that cannot be read as a date are
different defects, and the specification states an answer for neither, so the
run must fail and no artifact is accepted.

## How to fix

Correct `UNKNOWN` upstream when a date can be recovered. If invalid date text
is intentionally treated as no analysis date, declare that outcome locally:

```yaml
derivation:
  date_impute:
    source: AE.AESTDTC
    month: 1
    day: 1
    invalid: null
```

The `invalid` handler applies to non-missing text that is not an ISO 8601 date
or date prefix; it is distinct from the `missing` handler.
