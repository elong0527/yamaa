Following CDISC ADaM standards, use the provided ADVS dataset to create an ADVS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

The output holds every collected record and adds one body surface area
record for each subject and visit that has both a height result, coded
"HEIGHT", and a weight result, coded "WEIGHT". On a body surface area
record, PARAMCD is "BSA" and PARAM is "Body Surface Area (m^2)". On a
body surface area record, AVAL is the Mosteller value: the square root
of (height in centimeters times weight in kilograms divided by 3600),
and AVAL is not rounded. No body surface area record is added when the
visit's height or weight is absent or has no result. DTYPE is
"CALCULATION" on a body surface area record and has no value on a
collected record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
