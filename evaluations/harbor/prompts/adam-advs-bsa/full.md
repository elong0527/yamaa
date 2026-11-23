Following CDISC ADaM standards, use the provided ADVS dataset to create an ADVS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT

The output holds every collected record and adds one body surface area
record for each weight result, coded "WEIGHT", at the weight's visit.
Height, coded "HEIGHT", is collected once per subject, so every weight of
a subject uses the subject's height result, whatever visit collected it.
On a body surface area record, PARAMCD is "BSA" and PARAM is "Body
Surface Area (m^2)". On a body surface area record, AVAL is the
Mosteller value: the square root of (height in centimeters times weight
in kilograms divided by 3600), and AVAL is not rounded. No body surface
area record is added for a weight with no result, or when the subject
has no height result.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
