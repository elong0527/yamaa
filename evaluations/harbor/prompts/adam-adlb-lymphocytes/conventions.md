Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

The "LYMPH" parameter name is "Lymphocytes Abs (10^9/L)". DTYPE is CALCULATION
or empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
