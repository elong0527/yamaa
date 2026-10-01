Following CDISC ADaM standards, use the provided ADVS_RAW and LMS
datasets to create an ADVS dataset with one record per subject per
visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
