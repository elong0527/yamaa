# ADaM ADAE: reject an end date completed past the end of its month

This example uses collected adverse events whose end dates are sometimes
recorded without a day to attempt one record per event:

- `AENDT` is the end date of the event, completed to the latest date the
  collected text still allows.

The rule completes every partial end date to a thirty-first day, and one event
ended in a month of thirty. Moving the date into the following month, or back
to the last day of its own, would each change which month the event ended in,
so the run must fail and no artifact is accepted.

## How to fix

Choose a fixed day that is valid for every source month covered by the rule.
For example, an earliest-date policy can use:

```yaml
date_impute:
  source: AE.AEENDTC
  month: 12
  day: 1
```

If the analysis requires the actual last day of each month, one fixed `day`
cannot express that policy for months of different lengths. Supply complete,
calendar-valid end dates upstream or implement that separately defined rule
through the project's extension point.
