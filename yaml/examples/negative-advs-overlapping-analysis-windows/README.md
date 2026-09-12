# ADaM ADVS: reject overlapping analysis windows

This example uses one analysis record and a study-wide window table to attempt
one output record:

- `ADY` is the record's analysis day;
- `AVISIT` is meant to be the single analysis window containing that day.

The analysis day falls in two windows. Choosing either would make the result
depend on an unstated policy, so the run must fail rather than assign one.

## How to fix

First correct unintended overlap in the window table. If overlap is
intentional, state and justify a deterministic selection policy with
`order_by` and `keep`; do not rely on source-row order.
