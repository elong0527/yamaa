Following CDISC ADaM standards, use the provided ADSL dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI, BMI_FN

BMI is the body mass index: WEIGHTKG divided by the square of the height
in metres, where the height in metres is HEIGHTCM divided by 100. BMI has
no value when HEIGHTCM has no value or is zero, or when WEIGHTKG has no
value. A WEIGHTKG of zero gives a BMI of zero. BMI_FN is the same body
mass index, calculated by a project routine from HEIGHTCM and WEIGHTKG.
BMI_FN has no value when HEIGHTCM or WEIGHTKG has no value. BMI and
BMI_FN agree on every record. BMI and BMI_FN are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
