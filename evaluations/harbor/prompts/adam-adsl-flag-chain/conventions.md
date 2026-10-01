Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
POPFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, AGERNK, STUDYID, USUBJID,
AGE

SAFFL, ITTFL, and POPFL are each Y or N.

AGEGR1 holds <65 or >=65. AGERNK holds the subject rank by AGE within
the study, youngest first, with USUBJID breaking ties.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
