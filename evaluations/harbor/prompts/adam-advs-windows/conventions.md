Following CDISC ADaM standards, use the provided SV and VS datasets to create
an ADVS dataset with one record per vital signs record, plus an expected
record for each planned visit whose window holds no measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL, AVISIT,
AVISITN, ANL01FL

AVISITN is -1 for "SCREENING", 0 for "BASELINE", 2 for "WEEK 2", 4 for
"WEEK 4", and 99 for "POST-TREATMENT", or has no value. ANL01FL is "Y"
or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
