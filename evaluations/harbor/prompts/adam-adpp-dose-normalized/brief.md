Following CDISC ADaM standards, use the provided PP and EX datasets to
create an ADPP dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
