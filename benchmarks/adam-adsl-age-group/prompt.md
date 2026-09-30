Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AGE, AGEU, AGEGR1, AGEGR1N

Create AGEGR1 using these labels:
- <18
- 18-64
- >64
- Missing

Create AGEGR1N as the numeric code of AGEGR1: 1 for <18, 2 for 18-64, and
3 for >64. AGEGR1N has no value when AGEGR1 is Missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
