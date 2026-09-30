Following CDISC ADaM standards, use the provided ADVS_RAW dataset to
create an ADVS dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

Keep every collected record, and add one mean arterial pressure record
for each subject and visit that has both a systolic and a diastolic
blood pressure result. Mark it with PARAMCD "MAP" and PARAM
"Mean Arterial Pressure (mmHg)".

AVAL keeps each collected result. On the added record it is the
average that counts the diastolic pressure twice and the systolic
pressure once. No record is added when either contributor is absent
from the visit or its result is missing.

DTYPE is CALCULATION on the added record and has no value on collected
records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
