Following CDISC ADaM standards, use the provided LB dataset to create
an ADLBC dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG, PREV2

PARAMCD is "_ALB" for albumin.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlbc.csv.
