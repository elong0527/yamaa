# ADaM ADLB: reject an unnamed baseline-count rule

This example uses a pre-derived analysis slice to attempt one record per
subject, parameter, and analysis date:

- `AVAL` is the analysis value of the result;
- `ABLFL` is `Y` on the record that serves as the subject's baseline for the
  parameter, and is empty on every other record.

The specification asserts a clinical rule -- each subject and parameter has
exactly one baseline record -- without naming it. Counting rows within a group
is a study decision rather than a property of the data, so a report that only
said a count was wrong would not say which rule the data broke. The rule must
carry a name, so the run must fail before any data is read and no artifact is
accepted.

## How to fix

Name the rule the count asserts, in the words a reviewer would use for it:

```yaml
verifications:
  - row_count:
      id: one-baseline-record-per-subject-and-parameter
      group_by: [STUDYID, USUBJID, PARAMCD]
      filter: "ABLFL = 'Y'"
      min: 1
      max: 1
```

A count over the whole artifact rather than within a group asserts its size
instead of a study rule and needs no name, so `min` and `max` alone remain
valid there.
