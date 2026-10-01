Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per subject per visit per parameter,
including one total record per subject and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
