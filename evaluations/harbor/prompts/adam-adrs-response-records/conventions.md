Following CDISC ADaM standards, use the provided RS and ADSL datasets
to create an ADRS dataset with one record per investigator overall
response assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY, AVALC, AVAL,
ANL01FL

Use PARAMCD "OVR" and PARAM "Overall Response by Investigator".

AVALC is complete response (CR), partial response (PR), stable disease (SD),
neither complete response nor progressive disease (NON-CR/NON-PD), progressive
disease (PD), or not evaluable (NE). AVAL ranks the response from best to worst
as 1 (CR), 2 (PR), 3 (SD), 4 (NON-CR/NON-PD), 5 (PD), or 6 (NE).

ANL01FL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
