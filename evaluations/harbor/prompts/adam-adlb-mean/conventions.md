Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per collected laboratory record, plus one average
record per subject and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, LBSEQ, PARAMCD, AVAL, DTYPE

DTYPE is "AVERAGE" or has no value. AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
