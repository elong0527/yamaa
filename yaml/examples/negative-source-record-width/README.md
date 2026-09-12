# ADaM ADSL: reject a subject listing record with an extra field

This example uses a collected subject listing to attempt one record per
subject:

- `SITEID` is the site the subject enrolled at;
- `SEX` is the sex recorded at screening.

One record carries a field more than the listing names. Nothing says what the
surplus value is, so reading the record means either dropping a collected
value or shifting every value after it into the wrong field. The run must
fail and no artifact is accepted.

## How to fix

Export one field for each named field in every record. Where the extra value
is itself collected, name it in the header so that every record carries it:

    STUDYID,USUBJID,SITEID,SEX,SITENM

A bare separator inside a value produces the same surplus, so a value holding
a separator is quoted rather than left bare.
