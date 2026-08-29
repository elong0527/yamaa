# ADaM ADAE: reject an event start recorded as a moment in time

This example uses collected adverse events whose starts carry a time of day to
attempt one record per event:

- `ASTDTM` is meant to be the moment the event started.

A result holds text, whole numbers, fractional numbers, and calendar dates, and
a moment in time is none of them. Storing it as one of those without saying so
would leave two results that disagree about what the value is, so the run must
fail and no artifact is accepted. Text keeps the collected moment exactly and
orders chronologically.
