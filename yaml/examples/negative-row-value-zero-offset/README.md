# ADaM ADVS: reject a previous weight that names no earlier visit

This example uses collected weights to attempt one record per measurement:

- `AVAL` is the weight collected at the visit;
- `PREVAL` is meant to be the weight collected at the visit before it.

The rule states no distance to travel along the visit order, so it asks for the
visit itself. A record's own weight is already `AVAL`, and a second name for it
would let two spellings of one value drift apart, so the run must fail and no
artifact is accepted.

## How to fix

Use a negative offset to move to the preceding row in the declared ascending
visit order:

```yaml
row_value:
  source: AVAL
  offset: -1
  group_by: [STUDYID, USUBJID]
  order_by: [ADT, VSSEQ]
```

The first row in each subject partition then receives missing because it has no
preceding visit. If the current value is intended, reference `AVAL` directly
instead of using `row_value`.
