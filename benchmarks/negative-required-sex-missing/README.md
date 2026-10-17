# Reject a Demographics Record Without Sex

**Goal:** carry the collected sex (`SEX`) for each subject of the
demographics domain, where the tabulation standard requires it.

**Input:** collected demographics with the study and subject
identifiers and the recorded sex.

**Variables:**

- `SEX` would be the subject's recorded sex, copied from the collected
  records.

**Note:** in a tabulation dataset a required variable must carry a
value on every record. A subject whose sex was never recorded rejects
the run once the sex values are complete, so no artifact is accepted.

## How to fix

Lead with the data: the demographics form records sex for every
subject, so query the site and correct the record at its source. When
sex was asked but not known, record the study's code for unknown
(`U`) rather than leaving the value blank.

The requirement follows from the column's `core: Req` in a
`SPECIAL PURPOSE` dataset; a tabulation `Req` variable cannot be
relaxed with `mandatory: false`, so the fix belongs in the data.
