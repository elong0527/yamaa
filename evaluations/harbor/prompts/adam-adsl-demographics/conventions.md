Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SEX, SEXN, RACE, RACEN, AGE, AGEGR1

Code SEXN from SEX: 1 for M, 2 for F, and 0 for U.

RACEN is 1 for WHITE, 2 for BLACK OR AFRICAN AMERICAN, 3 for ASIAN, 4 for
MULTIPLE, or 99.

AGE is a whole number.

AGEGR1 is <18, 18-64, >=65, or UNKNOWN.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
