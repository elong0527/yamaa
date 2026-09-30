Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL
datasets to create an ADRS dataset with one record per subject per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS, AVALC,
ARSN, AVAL

Use PARAMCD "RESP75" and PARAM "EASI-75 Response".

The checks apply in a fixed order. A subject with a serious adverse
event or any discontinuation reason is a non-responder whatever the
efficacy value, even a missing one; otherwise a subject with no
efficacy value is not evaluable, one with a percent change of -75 or
less (a reduction of at least 75%) is a responder, and everyone else
is a non-responder.

AVALC is the responder status: RESPONDER, NON-RESPONDER, or NOT
EVALUABLE. ARSN records which check assigned the status: SAFETY OR
DISCONTINUATION RULE, COMPONENT MISSING, THRESHOLD MET, or THRESHOLD
NOT MET. AVAL is 1 for a responder and 0 for a non-responder; it has
no value when the subject is not evaluable.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
