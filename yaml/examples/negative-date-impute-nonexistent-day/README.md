# ADaM ADAE: reject an end date completed past the end of its month

This example uses collected adverse events whose end dates are sometimes
recorded without a day to attempt one record per event:

- `AENDT` is the end date of the event, completed to the latest date the
  collected text still allows.

The rule completes every partial end date to a thirty-first day, and one event
ended in a month of thirty. Moving the date into the following month, or back
to the last day of its own, would each change which month the event ended in,
so the run must fail and no artifact is accepted.
