Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, BRTHDT, RANDDT, AAGE, AAGEU

- AAGE is the count of yearly anniversaries of BRTHDT falling on or
  before RANDDT.
- AAGE has no value when BRTHDT or RANDDT has no value.
- A February 29 birthday falls on February 28 in a common year.
- When RANDDT is before BRTHDT, AAGE is the negated count of yearly
  anniversaries of RANDDT falling on or before BRTHDT.
- AAGEU is "YEARS" on every record, even when AAGE has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
