Following CDISC ADaM standards, use the provided ADVS dataset to create an ADVS
dataset with one record per subject, visit, and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

An added record with PARAMCD "MAP" and PARAM "Mean Arterial Pressure (mmHg)"
is created for each subject and visit that has both a systolic (SYSBP) and a
diastolic (DIABP) blood pressure result. On the added record, AVAL is
two-thirds of the diastolic pressure plus one-third of the systolic pressure.
AVAL on the added record is not rounded. No record is added when either
contributor is absent from the visit or its result is missing. DTYPE is
"CALCULATION" on the added record and has no value on collected records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
