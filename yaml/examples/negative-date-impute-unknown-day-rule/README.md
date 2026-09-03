# ADaM ADAE: reject a start date completed with an unrecognised day

This example uses collected adverse events whose start dates are sometimes
recorded without a day to attempt one record per event:

- `ASTDT` is the start date of the event, completed to a day chosen by a rule
  that names where in the month the date belongs.

The rule asks for the middle of the month. Only the first day and the last day
of a month are positions a calendar fixes for every month; a middle has no
single day in a month of 28, 30, or 31 days, so the run must fail and no
artifact is accepted rather than round in a direction nobody stated.

## How to fix

Name a position a calendar fixes, or name the day itself. To keep an event as
early as the collected text allows:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 6
  day: first
```

A value such as `2023-06` then becomes `2023-06-01`. Writing `day: last`
instead gives `2023-06-30`, and writing `day: 15` names the fifteenth of the
collected month directly.
