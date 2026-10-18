Following CDISC ADaM standards, use the provided QS dataset to create an ADQS
dataset with one record per subject, visit, and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL

The physical functioning score record has PARAMCD "F1SCORE" and PARAM
"Physical Functioning Scale Score". The role functioning score record has
PARAMCD "F2SCORE" and PARAM "Role Functioning Scale Score". The fatigue
and sleep symptom score record has PARAMCD "SSCORE" and PARAM "Fatigue and
Sleep Symptom Scale Score". The global health score record has PARAMCD
"GSCORE" and PARAM "Global Health Scale Score". AVAL on a score record is
not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
