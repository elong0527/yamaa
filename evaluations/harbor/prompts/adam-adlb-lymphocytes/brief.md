Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
