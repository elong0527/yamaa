Following CDISC ADaM standards, use the provided VS dataset to create
an ADVS dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL, AVALC

The parameters are:
- SYSBPOC, "Systolic Blood Pressure Change on Standing (mmHg)"
- DIABPOC, "Diastolic Blood Pressure Change on Standing (mmHg)"
- HROC, "Heart Rate Change on Standing (beats/min)"
- OHYPO, "Orthostatic Hypotension"
- OTACHY, "Orthostatic Tachycardia"

AVAL is not rounded. On a change record AVALC has no value; on a flag
record AVAL has no value and AVALC is "Y" or "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
