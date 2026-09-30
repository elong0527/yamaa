Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, R2ANRLO

R2ANRLO is the result as a multiple of the lower limit of the normal
range. It has no value when the result or the lower limit was not
collected, or the lower limit was recorded as zero.

Every number is reported to four places, so a result that needs fewer
still shows them and a ratio that needs more is rounded exactly once,
as it is written, with an exact half going away from zero (one
thirty-second is reported as 0.0313). Nothing is rounded before the
report: the ratio keeps every digit it was calculated with, and a
number that was never collected is reported as absent rather than as
four zeroes.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
