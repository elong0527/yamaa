# ADaM ADLB: reject a result with no reference range

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limit table has no entry for one test and sex that was collected, and the
specification states no answer for that case. Leaving the limit empty would
present an out-of-range result as unclassified rather than as unchecked, so the
run must fail and no artifact is accepted.
