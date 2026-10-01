Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, R2ANRLO

R2ANRLO is rounded once to four decimal places, with an exact half rounded away
from zero.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
