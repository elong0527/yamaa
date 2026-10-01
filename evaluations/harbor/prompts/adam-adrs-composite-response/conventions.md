Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL
datasets to create an ADRS dataset with one record per subject per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS, AVALC,
ARSN, AVAL

Use PARAMCD "RESP75" and PARAM "EASI-75 Response".

AVALC is RESPONDER, NON-RESPONDER, or NOT EVALUABLE. ARSN is SAFETY OR
DISCONTINUATION RULE, COMPONENT MISSING, THRESHOLD MET, or THRESHOLD NOT MET.
AVAL is 1 for a responder and 0 for a non-responder; it has no value when the
subject is not evaluable.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
