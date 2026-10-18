Following CDISC ADaM standards, use the provided ADSL and QS datasets to create
an ADQS dataset with one record per subject per parameter per analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL

The scheduled visits are "Week 8" (AVISITN 8), "Week 16" (AVISITN 16), and
"Week 24" (AVISITN 24). PARAMCD is "ACTOT". DTYPE is "LOCF" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
