Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, R2ANRLO

R2ANRLO is the analysis value as a multiple of the analysis normal range
lower limit: AVAL divided by ANRLO. R2ANRLO has no value when AVAL or ANRLO
has no value, or when ANRLO is zero. Every number is reported to four
decimal places, so a value that needs fewer places still shows them. A
value that needs more than four places is rounded exactly once, as it is
reported, with an exact half going away from zero: one thirty-second is
reported as 0.0313. Nothing is rounded before the report: the ratio keeps
every digit it was calculated with. A number that was never collected is
reported as absent rather than as four zeroes.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
