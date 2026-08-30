# ADaM ADSL: reject a body surface area from a negative weight

This example uses collected height and weight to attempt one record per
subject:

- `BSA` is body surface area in square metres, derived from the collected
  height and weight.

One weight carries a leading minus sign, so the area has no real value. Any
answer would be invented rather than derived, so the run must fail and no
artifact is accepted.
