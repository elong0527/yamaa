Following CDISC ADaM standards, use the provided AE, DM, DS, EX, SS, SV, and VS
datasets to create an ADSL dataset with one record per subject in DM.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LSTALVDT, DTHFL, DTHDT, DTHDTF

DTHFL is "Y" or has no value. DTHDTF is "D" when the day of DTHDT was
supplied, "M" when its month and day were, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
