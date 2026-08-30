# ADaM ADVS: reject a pulse rate recorded between whole beats

This example uses collected pulse rates to attempt one record per measurement:

- `AVAL` is the pulse in beats per minute, which the analysis holds as a whole
  number.

One rate was averaged over a half-minute count and recorded with a fraction.
Dropping the fraction and moving to the nearest whole number disagree, and both
report a rate that was not measured, so the run must fail and no artifact is
accepted.
