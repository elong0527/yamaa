Following CDISC ADaM standards, use the provided ADSL and QS datasets
to create an ADQS dataset with one record per subject per parameter
per analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL

Use PARAMCD "ACTOT". AVISIT "Week 8", "Week 16", and "Week 24" have AVISITN 8,
16, and 24.

DTYPE is "LOCF" or empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
