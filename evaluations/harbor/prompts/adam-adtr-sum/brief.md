Following CDISC ADaM standards, use the provided TRVISIT, TR, and TU
datasets to create an ADTR dataset with one record per subject per
scheduled tumor assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS,
NTARGET, ANL01FL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
