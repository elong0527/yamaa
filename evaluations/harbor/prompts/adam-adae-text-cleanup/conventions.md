Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESPID, AEREFNUM, AETERM, AETERMLO, AERELLC,
AREL

AEREFNUM is a number, or -1. AETERMLO is in lower case. AERELLC is in
lower case, or is "not reported". AREL is the analysis causality in upper
case: RELATED, POSSIBLY RELATED, NOT RELATED, or NOT REPORTED.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
