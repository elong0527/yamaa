Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
