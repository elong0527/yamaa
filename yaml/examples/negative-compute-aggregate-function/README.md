# ADaM ADLB: reject a total written as a formula

This example uses collected laboratory results to attempt one record per
subject and parameter:

- `AVAL` is the collected result;
- `AVALTOT` is meant to total the results across the records of a subject.

`AVALTOT` puts `SUM(AVAL)` in `compute`, but `compute` is a scalar expression:
it evaluates the current row only and does not permit aggregate functions such
as `SUM`. Validation therefore fails with `prohibited_function` before any
grouping or execution occurs.

The output `keys` do not become a default `group_by` for an unqualified
aggregate. If the intended result were a total for each subject, the derivation
would have to select the aggregate operation and declare that grain explicitly:

```yaml
derivation:
  aggregate:
    group_by: [STUDYID, USUBJID]
    expr: "SUM(AVAL)"
```

That result would be broadcast to each parameter row for the subject. The
negative specification declares neither the aggregate operation nor its grain,
so the run fails and no artifact is accepted.
