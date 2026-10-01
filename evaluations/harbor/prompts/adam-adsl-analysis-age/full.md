Following CDISC ADaM standards, use the provided DM dataset to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, BRTHDT, RANDDT, AAGE, AAGEU

AAGE is the count of yearly anniversaries of the birth date falling on
or before the randomization date; it has no value when either date is
absent. A February 29 birthday falls on February 28 in common years,
and a randomization date before the birth date gives the negated count
with the dates exchanged. AAGEU is YEARS on every record, even where
AAGE is missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
