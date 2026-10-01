Following CDISC ADaM standards, use the provided QS dataset to create
an ADQS dataset with one record per subject per visit per item, plus
one score record per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
