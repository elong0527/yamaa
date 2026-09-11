# ADaM ADLB: report each result as text at its own precision

This example uses collected lab results and produces one row per subject and
test:

- `AVAL` is the collected result, carrying every digit it was measured with,
  and is absent when the result was not collected;
- `AVALC` is that result written to two places, so a result needing fewer
  still shows them; it is absent whenever the result is;
- `ANRLO` is the lower limit of the normal range for the test;
- `R2ANRLO` is the result as a multiple of that lower limit, again carrying
  every digit, and is absent when either input is;
- `R2ANRLOC` is that ratio written to four places, beside a result written to
  two.

A written result is text and the number beside it is untouched, so the ratio
is calculated from the whole result rather than from the two places reported
for it. An exact half goes away from zero, which is why 5.125 is reported as
`5.13`; 2.675 is reported as `2.67` because the value a computer holds for it
is a shade below the half, and so is not one.
