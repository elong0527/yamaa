# ADaM ADLB: reject a total written as a formula

This example uses collected laboratory results to attempt one record per
subject and parameter:

- `AVAL` is the collected result;
- `AVALTOT` is meant to total the results across the records of a subject.

A formula describes one record at a time and cannot reach the other records it
would have to total. Assuming a group from the output identity would answer a
question nobody asked, so the run must fail and no artifact is accepted.

## How to fix

Use an aggregate and state the subject-level grain explicitly:

```yaml
derivation:
  aggregate:
    group_by: [STUDYID, USUBJID]
    expr: "SUM(AVAL)"
```

The total is then broadcast to each parameter row for that subject.
