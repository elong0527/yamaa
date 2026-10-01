Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
POPFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, AGERNK, STUDYID, USUBJID,
AGE

TRTSDT is the earliest dated exposure start for the subject, and has
no value when no exposure record carries a date. RANDDT is the
collected randomization date, and has no value when the subject was
not randomized.

SAFFL is Y when the subject has a first exposure date and N
otherwise. ITTFL is Y when the subject was randomized and N
otherwise. POPFL is Y only when both SAFFL and ITTFL hold Y, and N
otherwise.

AGEGR1 holds <65 when AGE is below 65 and >=65 when it is 65 or above.
AGERNK holds the subject rank by AGE within the study, youngest first,
with USUBJID breaking ties.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
