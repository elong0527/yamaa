Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
POPFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, AGERNK, STUDYID, USUBJID, AGE

TRTSDT is the subject's earliest dated exposure start, and has no value when
no exposure record for the subject carries a date. SAFFL is "Y" when the
subject has a first exposure date and "N" otherwise. RANDDT is the
randomization date collected for the subject, and has no value when the
subject was not randomized. ITTFL is "Y" when the subject was randomized and
"N" otherwise. POPFL is "Y" only when SAFFL and ITTFL are both "Y", and "N"
otherwise. AGEGR1 is "<65" when AGE is below 65 and ">=65" when AGE is 65 or
above. AGERNK is the subject's rank by AGE within the study, from 1 for the
youngest, with ties in AGE broken by USUBJID.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
