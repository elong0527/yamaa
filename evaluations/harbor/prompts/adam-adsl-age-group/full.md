Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AGE, AGEU, AGEGR1, AGEGR1N

AGEGR1 is "<18" when AGE is below 18, "18-64" when AGE is 18 through 64,
and ">64" when AGE is above 64.
AGEGR1 is "Missing" when AGE has no value.
AGEGR1N is 1 for "<18", 2 for "18-64", and 3 for ">64".
AGEGR1N has no value when AGE has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
