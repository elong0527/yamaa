# ADaM ADVS: reject a previous weight that names no earlier visit

This example uses collected weights to attempt one record per measurement:

- `AVAL` is the weight collected at the visit;
- `PREVAL` is meant to be the weight collected at the visit before it.

The rule states no distance to travel along the visit order, so it asks for the
visit itself. A record's own weight is already `AVAL`, and a second name for it
would let two spellings of one value drift apart, so the run must fail and no
artifact is accepted.
