Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, R2ANRLO

Every number is reported to four decimal places, so a value that needs
fewer places still shows them. A value that needs more than four places
is rounded exactly once, as it is reported, with an exact half going away
from zero: one thirty-second is reported as 0.0313. Nothing is rounded
before the report. A number that was never collected is reported as absent
rather than as four zeroes.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
