Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE

On the total record, PARAMCD is TOTAL and PARAM is "Total of Components".
AVAL is not rounded. DTYPE is CALCULATION or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
