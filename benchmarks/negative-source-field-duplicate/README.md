# Reject Duplicate Field

**Goal:** build a subject-level listing carrying `SITEID` and
`SEX`.

**Input:** collected demographics records for two subjects at one
site, carrying `SITEID` and two fields both named `SEX`. The two
fields agree for the first subject and disagree for the second, so
no single answer is defensible there.

**Variables:**

- `SITEID` would contain the site the subject enrolled at, copied
  from the collected records.
- `SEX` would contain the recorded sex, copied from the collected
  records, but a value asked for by that name could come from
  either of the two fields, which disagree for the second subject.

The run is rejected when the listing is read and no artifact is
accepted.

## How to fix

Give every field of the listing its own name. If the second field holds the
sex as it was first collected, name it for that, and read whichever one the
study governs:

    STUDYID,USUBJID,SITEID,SEX,SEXORIG

If the two fields hold the same fact twice, remove one at the source. A
listing that answers to one name twice cannot be read the same way twice.
