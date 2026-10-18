Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
POPFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, AGERNK, STUDYID, USUBJID, AGE

SAFFL is "Y" or "N". ITTFL is "Y" or "N". POPFL is "Y" or "N". AGEGR1 is
"<65" when AGE is below 65 and ">=65" when AGE is 65 or above. AGERNK is the
subject's rank by AGE within the study, from 1 for the youngest, with ties
in AGE broken by USUBJID.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
