Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL
datasets to create an ADRS dataset with one record per subject per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS, AVALC,
ARSN, AVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
