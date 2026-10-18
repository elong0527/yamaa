Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, PARAMCD, AVALC, AVAL, AVALU, DTYPE

DTYPE is "IMPUTATION" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
