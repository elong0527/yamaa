# SDTM LB: reject a result with no reference range

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per result:

- `LBSTNRHI` is the upper limit of normal for the test and sex of the result.

The limit table has no entry for one test and sex that was collected, and the
specification states no answer for that case. Leaving the limit empty would
present an out-of-range result as unclassified rather than as unchecked, so the
run must fail and no artifact is accepted.
