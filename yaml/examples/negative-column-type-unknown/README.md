# ADaM ADSL: reject a column with no stated kind of value

This example uses one collected demographics record to attempt one subject
record:

- `AGE` is meant to be the subject's age in years.

A result holds text, whole numbers, fractional numbers, calendar dates, or
moments in time, and every specification says which. `number` names two of
those at once and settles nothing: it leaves open whether a fractional age is
kept or rejected, and it would leave the next reader to guess. Naming a kind
outside the closed set must fail rather than resolve to whichever of the two a
reader assumed, so the run must fail and no artifact is accepted.
