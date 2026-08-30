# ADaM ADLB: reject a reference range chosen by an unpaired key

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limits are looked up by test and sex, but only the test is paired with a
column of the limit table. Dropping the unpaired value, or pairing it with a
column chosen by name, would each look up a different limit, so the run must
fail and no artifact is accepted.
