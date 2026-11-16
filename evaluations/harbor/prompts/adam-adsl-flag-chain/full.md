Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
FASFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, STUDYID, USUBJID, AGE

TRTSDT is the subject's earliest dated exposure start, and has no value when
no exposure record for the subject carries a date. SAFFL is "Y" when the
subject has a first exposure date and "N" otherwise. RANDDT is the
randomization date collected for the subject, and has no value when the
subject was not randomized. ITTFL is "Y" when the subject was randomized and
"N" otherwise. FASFL is "Y" only when SAFFL and ITTFL are both "Y", and "N"
otherwise. AGEGR1 is "<65" when AGE is below 65 and ">=65" when AGE is 65 or
above.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
