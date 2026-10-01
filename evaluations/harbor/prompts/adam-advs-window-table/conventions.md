Following CDISC ADaM standards, use the provided ADVS_RAW and AWINDOW
datasets to create an ADVS dataset with one record per measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, ADT, ADY, AVAL, AVISIT,
AVISITN, AWTARGET, AWTDIFF, ANL01FL

AWTDIFF is negative before the target and positive after.

ANL01FL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
