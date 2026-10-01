Following CDISC ADaM standards, use the provided ADSL and QS datasets
to create an ADQS dataset with one record per subject per parameter
per analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
