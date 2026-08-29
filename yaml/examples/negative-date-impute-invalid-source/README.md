# ADaM ADAE: reject a start date completed from text that is not a date

This example uses collected adverse events whose start dates are sometimes
incomplete to attempt one record per event:

- `ASTDT` is the start date of the event, completed from the earliest date the
  collected text still allows.

One start date was entered as a word rather than as a date or the beginning of
one. A date that was never collected and text that cannot be read as a date are
different defects, and the specification states an answer for neither, so the
run must fail and no artifact is accepted.
