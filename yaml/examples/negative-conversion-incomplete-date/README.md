# ADaM ADAE: reject an event start date that names no day

This example uses collected adverse events to attempt one record per event:

- `ASTDT` is the date the event started.

One start date was collected as a month without a day. A date identifies one
day, so choosing the first of the month, the last, or the middle would each
answer with a day nobody recorded. Completing a partial date is a rule a
specification states deliberately, and this one states none, so the run must
fail and no artifact is accepted.
