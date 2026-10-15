Following CDISC ADaM standards, use the provided EG dataset to create an
ADEG dataset with one record per subject per parameter per analysis time
point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
