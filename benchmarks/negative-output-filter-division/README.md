# Reject a Kept-Rows Filter That Divides by Zero

**Goal:** publish each subject's tests (`TEST`) whose value (`VAL`) is under
10, read as 100 divided by the value exceeding 10, with the subject (`SUBJ`).

**Input:** each input record is one test result carrying the subject, the
test and its integer value; subject `S1`'s test `T2` has the value 0.

**Variables:**

- `SUBJ`, `TEST` and `VAL` would carry the subject, the test and its value
  from the input record.

**Note:** the filter that chooses the published records divides 100 by the
value, and a value of 0 has no quotient, so the run fails at that record
instead of silently keeping or dropping it. No artifact is accepted.

## How to fix

State the comparison without dividing, so a zero value is simply compared:

```yaml
output:
  path: obs.csv
  columns: [SUBJ, TEST, VAL]
  filter: VAL > 0 AND VAL < 10
```

When the quotient itself is wanted, decide what a zero value means first, and
choose missing explicitly with `NULLIF(VAL, 0)`: a missing quotient makes the
comparison unknown, and an unknown filter drops the record.
