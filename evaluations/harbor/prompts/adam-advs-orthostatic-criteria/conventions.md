Following CDISC ADaM standards, use the provided VS dataset to create an ADVS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL, AVALC

PARAMCD and PARAM name the change or the flag the record holds: SYSBPOC is
"Systolic Blood Pressure Change on Standing (mmHg)", DIABPOC is "Diastolic
Blood Pressure Change on Standing (mmHg)", HROC is "Heart Rate Change on
Standing (beats/min)", OHYPO is "Orthostatic Hypotension", and OTACHY is
"Orthostatic Tachycardia".

On a change record, AVAL is not rounded. AVALC is "Y" or "N" on a flag
record and has no value on a change record, and AVAL has no value on a
flag record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
