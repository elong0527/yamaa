Following CDISC ADaM standards, use the provided ADSL and RS datasets to create
an ADRS dataset with one record per investigator-scored overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY, AVALC, AVAL, ANL01FL

PARAMCD is "OVR" and PARAM is "Overall Response by Investigator" on
every record. ADY is the study day of the assessment, counting the
treatment start as day one. AVALC is "CR", "PR", "SD", "NON-CR/NON-PD",
"PD", or "NE". The codes stand for complete response ("CR"), partial
response ("PR"), stable disease ("SD"), neither complete response nor
progressive disease ("NON-CR/NON-PD"), progressive disease ("PD"), and
not evaluable ("NE"). AVAL ranks the response from best to worst: 1 for
"CR", 2 for "PR", 3 for "SD", 4 for "NON-CR/NON-PD", 5 for "PD", and 6
for "NE". ANL01FL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
