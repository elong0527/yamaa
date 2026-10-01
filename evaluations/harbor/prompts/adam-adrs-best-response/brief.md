Following CDISC ADaM standards, use the provided ADSL and ADRSSEL
datasets to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
