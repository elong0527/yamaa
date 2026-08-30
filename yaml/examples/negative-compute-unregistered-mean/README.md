# ADaM ADLB: reject an average written as a formula

This example uses collected neutrophil results to attempt one record per
subject and parameter:

- `AVAL` is the collected result;
- `AVALMEAN` is meant to hold the subject's average result.

A formula describes one record at a time, and no function name widens it into
a reduction. The portable vocabulary names no average, so the run must fail
and no artifact is accepted.

## How to fix

Write the average from the two reductions the vocabulary does register, and
state the subject-level grain explicitly:

```yaml
derivation:
  aggregate:
    group_by: [STUDYID, USUBJID, PARAMCD]
    expr: "SUM(AVAL) / COUNT(AVAL)"
```

A result whose group contains no non-missing values is a division by zero and
fails at evaluation; a group of a fixed minimum size should be confirmed with
a row-count check instead of assumed.
