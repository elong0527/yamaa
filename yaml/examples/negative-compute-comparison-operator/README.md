# ADaM ADLB: reject an above-range flag written as a formula

This example uses collected laboratory results with their reference limits to
attempt one record per subject and parameter:

- `AVAL` is the collected result and `ANRHI` the upper limit of its reference
  range;
- `HIFL` is meant to mark a result above that limit.

A formula produces a number, and a comparison answers yes or no. Turning the
answer into one or zero, or into text, would each be a different result from
the same specification, so the run must fail and no artifact is accepted. A
comparison belongs where the specification asks a question rather than
calculates a value.
