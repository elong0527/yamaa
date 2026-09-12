# ADaM ADLB: report a result against the lower limit of normal

This example uses sample LB results and produces one row per subject and test:

- `AVAL` is the collected result;
- `ANRLO` is the lower limit of the normal range for the test;
- `R2ANRLO` is the result as a multiple of that lower limit. A test whose lower
  limit was not collected, or was recorded as zero, has no ratio.

Every number is reported to four places, so a result that needs fewer still
shows them and a ratio that needs more is rounded exactly once, as it is
written. An exact half goes away from zero, which is why a ratio of one
thirty-second is reported as `0.0313`. Nothing before the report sees a rounded
number: the ratio keeps every digit it was calculated with, and a number that
was never collected is reported as absent rather than as four zeroes.
