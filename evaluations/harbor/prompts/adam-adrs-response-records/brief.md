Following CDISC ADaM standards, use the provided RS and ADSL datasets
to create an ADRS dataset with one record per investigator overall
response assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY, AVALC, AVAL,
ANL01FL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
