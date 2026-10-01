Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per collected laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, PARAMCD, AVAL, AVALMEAN

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
