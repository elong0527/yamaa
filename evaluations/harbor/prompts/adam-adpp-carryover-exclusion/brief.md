Following CDISC ADaM standards, use the provided PP and PC datasets to
create an ADPP dataset with one record per PK parameter per subject per
period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISITNUM, VISIT, PARAMCD, PARAM, AVAL, PRECONC,
PREPCMAX, ANL01FL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
