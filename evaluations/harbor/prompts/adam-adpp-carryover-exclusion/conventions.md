Following CDISC ADaM standards, use the provided PP and PC datasets to
create an ADPP dataset with one record per PK parameter per subject per
period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISITNUM, VISIT, PARAMCD, PARAM, AVAL, PRECONC,
PREPCMAX, ANL01FL

PARAMCD and PARAM are the PP test code and test name. PREPCMAX is a
percentage, not rounded. ANL01FL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
