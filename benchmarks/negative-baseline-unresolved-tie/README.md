# Reject Baseline Tied on Date and Time

**Goal:** flag each subject's baseline result (`ABLFL`) as the result
collected last on or before treatment start.

**Input:** laboratory results carrying subject, sequence number
(`LBSEQ`), collection date (`ADT`) and time (`ATM`), result (`AVAL`),
and the subject's treatment start date (`TRTSDT`).

**Variables:**

- `ADT`, `ATM`, `AVAL` and `TRTSDT` would be carried through unchanged.
- `ABLFL` would be `Y` on the result with the latest date on or before
  treatment start, the latest collection time breaking a same-day tie,
  and blank on every other result.

**Note:** two results can share both the latest date and its collection
time, and either one gives the subject a different baseline. The run
fails and no dataset is accepted.

## How to fix

Decide which of the two results is the baseline before choosing how to
state it. Two results drawn at the same date and time are usually a sample
and its repeat, or one result recorded twice; the source data system should
carry the result the study reports, and correcting it there leaves the rule
saying what it means.

When both results are reportable and the study names a further tie-break,
add it after the collection time:

```yaml
baseline_flag:
  date: ADT
  reference_date: TRTSDT
  value: AVAL
  tie_break: [ATM, LBSEQ]
  window:
    group_by: [USUBJID]
```

Order by the term the study names; the sequence number above stands in for
it and is not itself a clinical rule. Do not average the two results into
one baseline value, which reports a measurement nobody took.
