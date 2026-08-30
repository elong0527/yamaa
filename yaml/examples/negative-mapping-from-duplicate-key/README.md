# ADaM ADLB: reject a reference range stated twice

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limit table gives two different upper limits for the same test and sex.
Taking either one, or the first the file happens to list, would make the result
depend on the order of a file rather than on the study's reference ranges, so
the run must fail and no artifact is accepted.
