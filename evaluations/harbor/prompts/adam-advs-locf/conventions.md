Following CDISC ADaM standards, use the provided VS dataset to create an
ADVS dataset with one record per subject per parameter per planned
visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVISITN, AVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
