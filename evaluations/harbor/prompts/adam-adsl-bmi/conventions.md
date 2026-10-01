Following CDISC ADaM standards, use the provided SOURCE dataset to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI, BMI_FN

BMI holds body mass index from weight in kilograms and height in centimetres,
with height converted from centimetres to metres, and is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
