# ADaM ADSL: reject a subject listing that names one field twice

This example uses a collected subject listing to attempt one record per
subject:

- `SITEID` is the site the subject enrolled at;
- `SEX` is the sex recorded at screening.

The listing carries two fields named `SEX`. A value asked for by that name
could come from either of them, so two readers of the same file can store
different sexes for the same subject and neither is wrong. The run must fail
and no artifact is accepted.

## How to fix

Give every field of the listing its own name. If the second field holds the
sex as it was first collected, name it for that, and read whichever one the
study governs:

    STUDYID,USUBJID,SITEID,SEX,SEXORIG

If the two fields hold the same fact twice, remove one at the source. A
listing that answers to one name twice cannot be read the same way twice.
