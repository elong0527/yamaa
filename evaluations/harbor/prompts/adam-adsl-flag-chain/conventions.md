Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
FASFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, STUDYID, USUBJID, AGE

SAFFL is "Y" or "N". ITTFL is "Y" or "N". FASFL is "Y" only when SAFFL and
ITTFL are both "Y", and "N" otherwise. AGEGR1 is "<65" when AGE is below 65
and ">=65" when AGE is 65 or above.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
