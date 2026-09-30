Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SEX, SEXN, RACE, RACEN, AGE, AGEGR1

Carry SEX through, using U when no sex was collected. Code SEXN from
SEX: 1 for M, 2 for F, and 0 for U.

Carry RACE through unchanged. Code RACEN by exact match: 1 for WHITE,
2 for BLACK OR AFRICAN AMERICAN, 3 for ASIAN, and 4 for MULTIPLE.
RACEN has no value when no race was collected; any other reported
value gives 99.

Keep AGE as the collected whole number; AGE has no value when no age
was collected or the reported value is not a whole number.

Group AGE into AGEGR1: <18 below 18, 18-64 from 18 to below 65, and
>=65 from 65 on. AGEGR1 is UNKNOWN when AGE has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
