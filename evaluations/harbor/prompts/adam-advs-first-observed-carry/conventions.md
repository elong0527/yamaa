Following CDISC ADaM standards, use the provided VS dataset to create an
ADVS dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, BASEVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
