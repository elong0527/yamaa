Following CDISC ADaM standards, use the provided ADVS_RAW dataset to
create an ADVS dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

Keep every collected record, and add one body surface area record for
each subject and visit that has both a height and a weight result. Mark
it with PARAMCD "BSA" and PARAM "Body Surface Area (m^2)".

AVAL keeps each collected result. On a body surface area record it is
the Mosteller value, the square root of the visit's height in
centimeters times its weight in kilograms divided by 3600, not rounded;
a zero result still counts as a result. No record is added when the
visit's height or weight is absent or has no result.

DTYPE is CALCULATION on a body surface area record and has no value on
a collected record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
