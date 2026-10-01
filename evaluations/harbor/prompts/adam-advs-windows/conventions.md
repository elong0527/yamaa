Following CDISC ADaM standards, use the provided ADVS_RAW dataset to
create an ADVS dataset with one record per measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL,
AVISIT, AVISITN, ANL01FL

AVISIT is SCREENING, BASELINE, WEEK 2, WEEK 4, or POST-TREATMENT, or has no
value.

AVISITN is the numeric order of the analysis visit: -1 for SCREENING,
0 for BASELINE, 2 for WEEK 2, 4 for WEEK 4, and 99 for POST-TREATMENT.

ANL01FL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
